use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use tree_sitter::{Node, Parser, Tree};

use super::ParseError;
use super::language::LanguageParser;
use super::patterns::FfiPatterns;
use crate::ir::{
    BinaryOp, Expression, Literal, Param, PointerType, Statement, StatusCheckKind, UnitKind, VarId,
    VarIdGenerator, VarName, VerifyUnit,
};
use crate::source::{ByteLength, ByteOffset, SourceFile, SourceSpan};

pub struct SwiftParser {
    parser: Parser,
    patterns: FfiPatterns,
}

impl SwiftParser {
    pub fn new() -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        let language = tree_sitter_swift::LANGUAGE;
        parser
            .set_language(&language.into())
            .map_err(|e| ParseError::SyntaxError {
                message: format!("failed to set Swift language: {}", e),
            })?;
        Ok(Self {
            parser,
            patterns: FfiPatterns::swift(),
        })
    }
}

impl LanguageParser for SwiftParser {
    fn language_name(&self) -> &'static str {
        "Swift"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["swift"]
    }

    fn parse_source(&mut self, path: &Path, source: &str) -> Result<Vec<VerifyUnit>, ParseError> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| ParseError::SyntaxError {
                message: "failed to parse Swift source".to_string(),
            })?;

        let source_file = Arc::new(SourceFile::new(path, source));
        let extractor = SwiftExtractor::new(source_file, source.to_string(), self.patterns.clone());
        extractor.extract_units(&tree)
    }
}

impl Default for SwiftParser {
    fn default() -> Self {
        Self::new().expect("failed to create Swift parser")
    }
}

struct SwiftExtractor {
    source_file: Arc<SourceFile>,
    source: String,
    patterns: FfiPatterns,
}

impl SwiftExtractor {
    fn new(source_file: Arc<SourceFile>, source: String, patterns: FfiPatterns) -> Self {
        Self {
            source_file,
            source,
            patterns,
        }
    }

    fn extract_units(self, tree: &Tree) -> Result<Vec<VerifyUnit>, ParseError> {
        let root = tree.root_node();
        let mut units = Vec::new();

        self.collect_functions(root, &mut units, None);

        Ok(units)
    }

    fn collect_functions(&self, node: Node, units: &mut Vec<VerifyUnit>, class_name: Option<&str>) {
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_declaration" => {
                    if let Some(unit) = self.extract_function(child, class_name) {
                        units.push(unit);
                    }
                }
                "init_declaration" => {
                    if let Some(class) = class_name
                        && let Some(unit) = self.extract_initializer(child, class)
                    {
                        units.push(unit);
                    }
                }
                "deinit_declaration" => {
                    if let Some(class) = class_name
                        && let Some(unit) = self.extract_deinitializer(child, class)
                    {
                        units.push(unit);
                    }
                }
                "class_declaration" => {
                    let name = self
                        .find_identifier(child)
                        .unwrap_or_else(|| "Unknown".to_string());
                    self.collect_functions(child, units, Some(&name));
                }
                "class_body" => {
                    self.collect_functions(child, units, class_name);
                }
                _ => {}
            }
        }
    }

    fn extract_function(&self, node: Node, class_name: Option<&str>) -> Option<VerifyUnit> {
        let name = self.find_identifier(node)?;

        let mut ctx = ExtractionContext::new();
        let params = self.extract_parameters(node, &mut ctx);

        let body_node = self.find_child_by_kind(node, "function_body")?;
        let statements_node = self.find_child_by_kind(body_node, "statements")?;
        let body = self.extract_statements(statements_node, &mut ctx);

        let kind = match class_name {
            Some(c) => UnitKind::Method {
                class_name: c.to_string(),
            },
            None => UnitKind::FreeFunction,
        };

        Some(VerifyUnit {
            name,
            kind,
            params,
            body,
            span: self.node_span(node),
        })
    }

    fn extract_initializer(&self, node: Node, class_name: &str) -> Option<VerifyUnit> {
        let mut ctx = ExtractionContext::new();
        let params = self.extract_parameters(node, &mut ctx);

        let body_node = self.find_child_by_kind(node, "function_body")?;
        let statements_node = self.find_child_by_kind(body_node, "statements")?;
        let body = self.extract_statements(statements_node, &mut ctx);

        Some(VerifyUnit {
            name: "init".to_string(),
            kind: UnitKind::Initializer {
                class_name: class_name.to_string(),
            },
            params,
            body,
            span: self.node_span(node),
        })
    }

    fn extract_deinitializer(&self, node: Node, class_name: &str) -> Option<VerifyUnit> {
        let mut ctx = ExtractionContext::new();

        let body_node = self.find_child_by_kind(node, "function_body")?;
        let statements_node = self.find_child_by_kind(body_node, "statements")?;
        let body = self.extract_statements(statements_node, &mut ctx);

        Some(VerifyUnit {
            name: "deinit".to_string(),
            kind: UnitKind::Deinitializer {
                class_name: class_name.to_string(),
            },
            params: vec![],
            body,
            span: self.node_span(node),
        })
    }

    fn extract_parameters(&self, func_node: Node, ctx: &mut ExtractionContext) -> Vec<Param> {
        let Some(params_node) = self.find_child_by_kind(func_node, "parameter") else {
            return vec![];
        };

        let mut cursor = params_node.walk();
        params_node
            .children(&mut cursor)
            .filter(|p| p.kind() == "parameter")
            .filter_map(|param| {
                let name = self.find_identifier(param)?;
                let param_type = self
                    .find_child_by_kind(param, "type_annotation")
                    .map(|t| self.node_text(t))
                    .unwrap_or_else(|| "Any".to_string());

                let var_id = ctx.new_var(&name);

                Some(Param {
                    name: VarName::new(name),
                    var_id,
                    param_type,
                    span: self.node_span(param),
                })
            })
            .collect()
    }

    fn extract_statements(
        &self,
        statements_node: Node,
        ctx: &mut ExtractionContext,
    ) -> Vec<Statement> {
        let mut cursor = statements_node.walk();
        statements_node
            .children(&mut cursor)
            .filter_map(|child| self.extract_statement(child, ctx))
            .collect()
    }

    fn extract_statement(&self, node: Node, ctx: &mut ExtractionContext) -> Option<Statement> {
        match node.kind() {
            "property_declaration" => self.extract_property_declaration(node, ctx),
            "assignment" => self.extract_assignment(node, ctx),
            "call_expression" => self.extract_call_statement(node, ctx),
            "if_statement" => self.extract_if_statement(node, ctx),
            "guard_statement" => self.extract_guard_statement(node, ctx),
            "return_statement" => self.extract_return_statement(node, ctx),
            "defer_statement" => self.extract_defer_statement(node, ctx),
            "statements" => self.extract_statements(node, ctx).into_iter().next(),
            _ => Some(Statement::Other {
                description: format!("unhandled: {}", node.kind()),
                span: self.node_span(node),
            }),
        }
    }

    fn extract_property_declaration(
        &self,
        node: Node,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let text = self.node_text(node);
        let is_let = text.starts_with("let");

        let name = self
            .find_child_by_kind(node, "pattern")
            .map(|n| self.node_text(n))?;

        let var_id = ctx.new_var(&name);

        if self.patterns.is_allocate(&text) {
            return self.extract_allocate_statement(node, var_id, &text);
        }

        if self.patterns.is_retain(&text) {
            return self.extract_pass_retained_statement(node, var_id, &text, ctx);
        }

        let value = self
            .find_descendant_by_kind(node, "call_expression")
            .or_else(|| self.find_descendant_by_kind(node, "navigation_expression"))
            .map(|v| self.extract_expression(v, ctx))
            .unwrap_or(Expression::Other {
                description: "no value".to_string(),
            });

        let span = self.node_span(node);

        if is_let {
            Some(Statement::LetBinding {
                var_id,
                name: VarName::new(name),
                value,
                span,
            })
        } else {
            Some(Statement::VarBinding {
                var_id,
                name: VarName::new(name),
                value: Some(value),
                span,
            })
        }
    }

    fn extract_assignment(&self, node: Node, ctx: &mut ExtractionContext) -> Option<Statement> {
        let target_node = node.child(0)?;
        let target_name = self.node_text(target_node);
        let target = ctx.get_or_create_var(&target_name);

        let value_node = node.child(2)?;
        let value = self.extract_expression(value_node, ctx);

        Some(Statement::Assignment {
            target,
            value,
            span: self.node_span(node),
        })
    }

    fn extract_call_statement(&self, node: Node, ctx: &mut ExtractionContext) -> Option<Statement> {
        let call_text = self.node_text(node);

        if self.patterns.is_defer(&call_text) {
            let body_start = call_text.find('{')? + 1;
            let body_end = call_text.rfind('}')?;
            let _body_text = &call_text[body_start..body_end];

            let body = self
                .find_child_by_kind(node, "lambda_literal")
                .or_else(|| self.find_descendant_by_kind(node, "statements"))
                .map(|s| self.extract_statements(s, ctx))
                .unwrap_or_default();

            return Some(Statement::Defer {
                body,
                span: self.node_span(node),
            });
        }

        if self.patterns.is_deallocate(&call_text) {
            let ptr_name = call_text.split('.').next()?.trim();
            let pointer_var = ctx.get_var(ptr_name)?;
            return Some(Statement::Deallocate {
                pointer_var,
                span: self.node_span(node),
            });
        }

        if self.patterns.is_status_check(&call_text) {
            return self.extract_status_check_statement(node, &call_text, ctx);
        }

        if self.patterns.is_release(&call_text) {
            let opaque_name = if call_text.contains("fromOpaque(") {
                call_text
                    .split("fromOpaque(")
                    .nth(1)
                    .and_then(|s| s.split(')').next())
                    .unwrap_or("")
                    .trim()
            } else {
                call_text.split('.').next().unwrap_or("").trim()
            };

            let opaque_var = ctx.get_var(opaque_name)?;
            return Some(Statement::Release {
                opaque_var,
                span: self.node_span(node),
            });
        }

        if call_text.contains("takeRetainedValue") {
            return self.extract_take_retained_statement(node, &call_text, ctx);
        }

        if self.is_ffi_call(&call_text) {
            return self.extract_ffi_call_statement(node, &call_text);
        }

        Some(Statement::Expression {
            expression: self.extract_expression(node, ctx),
            span: self.node_span(node),
        })
    }

    fn extract_if_statement(&self, node: Node, ctx: &mut ExtractionContext) -> Option<Statement> {
        let condition = self
            .find_child_by_kind(node, "if_condition_sequence_item")
            .map(|c| self.extract_expression(c, ctx))
            .unwrap_or(Expression::Other {
                description: "condition".to_string(),
            });

        let then_branch = self
            .find_child_by_kind(node, "statements")
            .map(|s| self.extract_statements(s, ctx))
            .unwrap_or_default();

        let else_branch = self
            .find_child_by_kind(node, "else")
            .and_then(|e| self.find_child_by_kind(e, "statements"))
            .map(|s| self.extract_statements(s, ctx));

        Some(Statement::IfStatement {
            condition,
            then_branch,
            else_branch,
            span: self.node_span(node),
        })
    }

    fn extract_guard_statement(
        &self,
        node: Node,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let condition = self
            .find_child_by_kind(node, "condition")
            .map(|c| self.extract_expression(c, ctx))
            .unwrap_or(Expression::Other {
                description: "guard".to_string(),
            });

        let else_branch = self
            .find_child_by_kind(node, "statements")
            .map(|s| self.extract_statements(s, ctx))
            .unwrap_or_default();

        Some(Statement::IfStatement {
            condition,
            then_branch: vec![],
            else_branch: Some(else_branch),
            span: self.node_span(node),
        })
    }

    fn extract_return_statement(
        &self,
        node: Node,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let value = node.child(1).map(|v| self.extract_expression(v, ctx));
        Some(Statement::Return {
            value,
            span: self.node_span(node),
        })
    }

    fn extract_defer_statement(
        &self,
        node: Node,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let body = self
            .find_child_by_kind(node, "statements")
            .map(|s| self.extract_statements(s, ctx))
            .unwrap_or_default();

        Some(Statement::Defer {
            body,
            span: self.node_span(node),
        })
    }

    fn extract_allocate_statement(
        &self,
        node: Node,
        var_id: VarId,
        text: &str,
    ) -> Option<Statement> {
        let element_type = text
            .split('<')
            .nth(1)
            .and_then(|s| s.split('>').next())
            .unwrap_or("Unknown")
            .to_string();

        let capacity = self.extract_capacity_from_text(text);

        Some(Statement::Allocate {
            target_var: var_id,
            pointer_type: if text.contains("Mutable") {
                PointerType::Mutable
            } else {
                PointerType::Immutable
            },
            element_type,
            capacity,
            span: self.node_span(node),
        })
    }

    fn extract_capacity_from_text(&self, text: &str) -> Expression {
        text.split("capacity:")
            .nth(1)
            .and_then(|s| {
                let trimmed = s.trim();
                let end = trimmed.find([')', ','])?;
                let cap_str = trimmed[..end].trim();
                cap_str
                    .parse::<i64>()
                    .ok()
                    .map(|n| Expression::Literal(Literal::Integer(n)))
                    .or_else(|| {
                        Some(Expression::Other {
                            description: cap_str.to_string(),
                        })
                    })
            })
            .unwrap_or(Expression::Other {
                description: "unknown".to_string(),
            })
    }

    fn extract_pass_retained_statement(
        &self,
        node: Node,
        opaque_var: VarId,
        text: &str,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let object_name = text
            .split("passRetained(")
            .nth(1)
            .and_then(|s| s.split(')').next())
            .unwrap_or("")
            .trim();

        let object_var = ctx.get_or_create_var(object_name);

        Some(Statement::PassRetained {
            object_var,
            opaque_var,
            span: self.node_span(node),
        })
    }

    fn extract_take_retained_statement(
        &self,
        node: Node,
        text: &str,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let opaque_name = text
            .split("fromOpaque(")
            .nth(1)
            .and_then(|s| s.split(')').next())
            .unwrap_or("")
            .trim();

        let opaque_var = ctx.get_or_create_var(opaque_name);
        let result_var = ctx.next_var();

        Some(Statement::TakeRetainedValue {
            opaque_var,
            result_var,
            span: self.node_span(node),
        })
    }

    fn extract_status_check_statement(
        &self,
        node: Node,
        text: &str,
        ctx: &mut ExtractionContext,
    ) -> Option<Statement> {
        let status_name = text
            .split(if text.contains("checkStatus(") {
                "checkStatus("
            } else {
                "ensureOk("
            })
            .nth(1)
            .and_then(|s| s.split(')').next())?
            .trim();

        let status_var = ctx.get_or_create_var(status_name);
        let check_kind = if text.contains("try ") {
            StatusCheckKind::TryCheckStatus
        } else {
            StatusCheckKind::EnsureOk
        };

        Some(Statement::StatusCheck {
            status_var,
            check_kind,
            span: self.node_span(node),
        })
    }

    fn extract_ffi_call_statement(&self, node: Node, text: &str) -> Option<Statement> {
        let function_name = text.trim().split('(').next()?.to_string();

        Some(Statement::FfiCall {
            function_name,
            arguments: vec![],
            result_var: None,
            out_params: vec![],
            span: self.node_span(node),
        })
    }

    fn extract_expression(&self, node: Node, ctx: &ExtractionContext) -> Expression {
        match node.kind() {
            "integer_literal" => self
                .node_text(node)
                .parse::<i64>()
                .map(|n| Expression::Literal(Literal::Integer(n)))
                .unwrap_or(Expression::Other {
                    description: self.node_text(node),
                }),
            "real_literal" => self
                .node_text(node)
                .parse::<f64>()
                .map(|n| Expression::Literal(Literal::Float(n)))
                .unwrap_or(Expression::Other {
                    description: self.node_text(node),
                }),
            "boolean_literal" => Expression::Literal(Literal::Bool(self.node_text(node) == "true")),
            "line_string_literal" => {
                let text = self.node_text(node);
                Expression::Literal(Literal::String(text.trim_matches('"').to_string()))
            }
            "nil" => Expression::Literal(Literal::Nil),
            "simple_identifier" => {
                let name = self.node_text(node);
                ctx.get_var(&name)
                    .map(Expression::Variable)
                    .unwrap_or(Expression::Other { description: name })
            }
            "call_expression" => {
                let text = self.node_text(node);
                if self.is_ffi_call(&text) {
                    Expression::FfiCallExpr {
                        function_name: text.split('(').next().unwrap_or(&text).to_string(),
                        arguments: vec![],
                    }
                } else {
                    Expression::Other { description: text }
                }
            }
            "prefix_expression" => {
                let text = self.node_text(node);
                if let Some(stripped) = text.strip_prefix('&') {
                    ctx.get_var(stripped.trim())
                        .map(Expression::AddressOf)
                        .unwrap_or(Expression::Other { description: text })
                } else {
                    Expression::Other { description: text }
                }
            }
            "infix_expression" | "comparison_expression" => {
                self.extract_binary_expression(node, ctx)
            }
            _ => Expression::Other {
                description: self.node_text(node),
            },
        }
    }

    fn extract_binary_expression(&self, node: Node, ctx: &ExtractionContext) -> Expression {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        if children.len() >= 3 {
            let left = self.extract_expression(children[0], ctx);
            let op_text = self.node_text(children[1]);
            let right = self.extract_expression(children[2], ctx);

            let operator = match op_text.as_str() {
                "+" => BinaryOp::Add,
                "-" => BinaryOp::Subtract,
                "*" => BinaryOp::Multiply,
                "/" => BinaryOp::Divide,
                "%" => BinaryOp::Modulo,
                "==" => BinaryOp::Equal,
                "!=" => BinaryOp::NotEqual,
                "<" => BinaryOp::LessThan,
                "<=" => BinaryOp::LessThanOrEqual,
                ">" => BinaryOp::GreaterThan,
                ">=" => BinaryOp::GreaterThanOrEqual,
                "&&" => BinaryOp::LogicalAnd,
                "||" => BinaryOp::LogicalOr,
                _ => {
                    return Expression::Other {
                        description: self.node_text(node),
                    };
                }
            };

            return Expression::BinaryOperation {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }

        Expression::Other {
            description: self.node_text(node),
        }
    }

    fn is_ffi_call(&self, text: &str) -> bool {
        self.patterns.is_ffi_call(text)
    }

    fn find_identifier(&self, node: Node) -> Option<String> {
        self.find_child_by_kind(node, "simple_identifier")
            .map(|n| self.node_text(n))
    }

    fn find_child_by_kind<'a>(&self, node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children.into_iter().find(|c| c.kind() == kind)
    }

    fn find_descendant_by_kind<'a>(&self, node: Node<'a>, kind: &str) -> Option<Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        children
            .into_iter()
            .find_map(|child| self.find_descendant_by_kind(child, kind))
    }

    fn node_text(&self, node: Node) -> String {
        self.source[node.start_byte()..node.end_byte()].to_string()
    }

    fn node_span(&self, node: Node) -> SourceSpan {
        SourceSpan::new(
            Arc::clone(&self.source_file),
            ByteOffset::from(node.start_byte()),
            ByteLength::from(node.end_byte() - node.start_byte()),
        )
    }
}

struct ExtractionContext {
    var_generator: VarIdGenerator,
    var_map: HashMap<String, VarId>,
}

impl ExtractionContext {
    fn new() -> Self {
        Self {
            var_generator: VarIdGenerator::new(),
            var_map: HashMap::new(),
        }
    }

    fn new_var(&mut self, name: &str) -> VarId {
        let id = self.var_generator.next();
        self.var_map.insert(name.to_string(), id);
        id
    }

    fn next_var(&mut self) -> VarId {
        self.var_generator.next()
    }

    fn get_var(&self, name: &str) -> Option<VarId> {
        self.var_map.get(name).copied()
    }

    fn get_or_create_var(&mut self, name: &str) -> VarId {
        if let Some(&id) = self.var_map.get(name) {
            id
        } else {
            self.new_var(name)
        }
    }
}

#[cfg(all(test, not(miri)))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let source = r#"
public func add(a: Int32, b: Int32) -> Int32 {
    return a + b
}
"#;
        let mut parser = SwiftParser::new().unwrap();
        let units = parser
            .parse_source(Path::new("test.swift"), source)
            .unwrap();

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].name, "add");
        assert!(matches!(units[0].kind, UnitKind::FreeFunction));
    }

    #[test]
    fn test_parse_function_with_defer() {
        let source = r#"
public func test() {
    let ptr = UnsafeMutablePointer<Int32>.allocate(capacity: 10)
    defer { ptr.deallocate() }
}
"#;
        let mut parser = SwiftParser::new().unwrap();
        let units = parser
            .parse_source(Path::new("test.swift"), source)
            .unwrap();

        assert_eq!(units.len(), 1);

        let has_alloc = units[0]
            .body
            .iter()
            .any(|s| matches!(s, Statement::Allocate { .. }));
        let has_defer = units[0]
            .body
            .iter()
            .any(|s| matches!(s, Statement::Defer { .. }));

        assert!(has_alloc, "Should have allocate statement");
        assert!(has_defer, "Should have defer statement");
    }

    #[test]
    fn test_parse_class_with_methods() {
        let source = r#"
public class MyClass {
    public func doSomething() {
        return
    }

    deinit {
        _ = boltffi_free(handle)
    }
}
"#;
        let mut parser = SwiftParser::new().unwrap();
        let units = parser
            .parse_source(Path::new("test.swift"), source)
            .unwrap();

        assert!(!units.is_empty());
    }
}
