use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: pim_reader <artifact> [--json]");
        return ExitCode::FAILURE;
    };
    let json = args.next().is_some_and(|flag| flag == "--json");

    let records = match pim_reader::read_artifact(&path) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };

    let resolved = match pim_reader::resolve(&records) {
        Ok(resolved) => resolved,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let items = resolved.items;

    if json {
        for record in &records {
            println!(
                "{} {:?} {}",
                record.module,
                record.slots,
                String::from_utf8_lossy(&record.json)
            );
        }
        return ExitCode::SUCCESS;
    }

    let width = items
        .iter()
        .flat_map(|item| item.fields.iter())
        .map(|field| field.name.len())
        .max()
        .unwrap_or(0);

    println!(
        "{} record(s), {} function(s), {} class(es), {} callback(s), {} stream(s) in {}",
        items.len(),
        resolved.functions.len(),
        resolved.classes.len(),
        resolved.callbacks.len(),
        resolved.streams.len(),
        path.display()
    );
    for item in &items {
        println!(
            "\n{}{}",
            item.canonical_id,
            if item.direct { "  [direct]" } else { "" }
        );
        for field in &item.fields {
            println!("  {:width$}  {}", field.name, field.ty);
        }
        if item.fields.is_empty() {
            println!("  (no fields)");
        }
    }

    for class in &resolved.classes {
        println!("\nclass {}", class.canonical_id);
        for method in class.constructors.iter().chain(&class.methods) {
            println!("  {}  {}", method.name, method.symbol);
        }
        println!("  free  {}", class.free_symbol);
    }

    for callback in &resolved.callbacks {
        println!("\ncallback {}", callback.canonical_id);
        for method in &callback.methods {
            println!("  {}", method.name);
        }
    }

    for stream in &resolved.streams {
        println!("\nstream {} of {}", stream.canonical_id, stream.item);
        println!(
            "  subscribe {}\n  pop {}\n  free {}",
            stream.subscribe_symbol, stream.pop_symbol, stream.free_symbol
        );
    }

    for scaffolding in &resolved.scaffolding {
        println!(
            "\nscaffolding {}\n  free {}",
            scaffolding.module, scaffolding.free_symbol
        );
    }

    for function in &resolved.functions {
        let params = function
            .params
            .iter()
            .map(|param| format!("{}: {}", param.name, param.ty))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = function
            .ret
            .as_deref()
            .map(|ty| format!(" -> {ty}"))
            .unwrap_or_default();
        println!("\nfn {}({params}){ret}", function.canonical_id);
        println!("  symbol {}", function.symbol);
    }

    ExitCode::SUCCESS
}
