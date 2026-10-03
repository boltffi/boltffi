import ast
import inspect
from pathlib import Path
from unittest.mock import patch

import demo

from tests.support import AsyncDemoTestCase, DemoTestCase


class DoublingCallback:
    def on_value(self, value: int) -> int:
        return value * 2


class DefaultArgumentsTests(DemoTestCase):
    def test_scalar_and_string_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_omitted_scalar_and_string_defaults")
        self.assertEqual(demo.repeat_greeting("ada"), "hello ada, hello ada")
        self.assertEqual(demo.repeat_greeting("ada", shout=True), "HELLO ADA, HELLO ADA")
        self.assertEqual(demo.repeat_greeting("ada", "hi", 1, True), "HI ADA")
        self.assertEqual(demo.repeat_greeting(name="ada", times=1), "hello ada")
        with self.assertRaises(TypeError):
            demo.repeat_greeting()
        with self.assertRaises(TypeError):
            demo.repeat_greeting("ada", name="other")

    def test_optional_defaults_preserve_explicit_none(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_none_and_value_defaults_to_optionals")
        self.assertEqual(demo.describe_limit(), "none:7")
        self.assertEqual(demo.describe_limit(label="jobs"), "jobs:7")
        self.assertEqual(demo.describe_limit(limit=None), "none:unlimited")
        self.assertEqual(demo.describe_limit("jobs", 2), "jobs:2")

    def test_optional_callbacks_and_closures(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_default_an_optional_callback_to_none")
        self.assertEqual(demo.apply_optional_callback(4), 4)
        self.assertEqual(demo.apply_optional_callback(4, callback=None), 4)
        self.assertEqual(demo.apply_optional_callback(4, callback=DoublingCallback()), 8)
        self.assertEqual(demo.apply_optional_closure(4), 4)
        self.assertEqual(demo.apply_optional_closure(4, callback=None), 4)
        self.assertEqual(demo.apply_optional_closure(4, callback=lambda value: value * 3), 12)
        with self.assertRaises(TypeError):
            demo.apply_optional_closure(4, callback=5)

    def test_constructor_and_method_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.defaulted_counter.should_apply_constructor_and_method_defaults")
        self.assertEqual(demo.DefaultedCounter().offset(), 11)
        self.assertEqual(demo.DefaultedCounter(5).offset(3), 8)
        self.assertEqual(demo.DefaultedCounter(start=5).offset(step=3), 8)
        self.assertEqual(demo.DefaultedCounter.from_text().offset(), 41)
        self.assertEqual(demo.DefaultedCounter.from_text(start="5").offset(), 6)
        self.assertEqual(demo.DefaultedCounter.with_offset(offset=3).offset(), 24)
        self.assertEqual(demo.DefaultedCounter.with_offset(5, 3).offset(), 9)
        self.assertEqual(demo.DefaultedCounter.with_offset(5, offset=3).offset(), 9)
        with self.assertRaisesRegex(TypeError, "missing required argument 'offset'"):
            demo.DefaultedCounter.with_offset()
        with self.assertRaisesRegex(TypeError, "missing required argument 'offset'"):
            demo.DefaultedCounter.with_offset(5)

    def test_float_and_enum_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_float_and_enum_defaults")
        self.assertEqual(demo.scale_default(), 0.75)
        self.assertEqual(demo.scale_default(weight=None), 0.5)
        self.assertEqual(demo.scale_default(mode=demo.DefaultMode.LOUD), 1.5)
        self.assertEqual(demo.scale_default(0.25, 2.0, demo.DefaultMode.LOUD), 1.0)
        self.assertTrue(demo.DefaultMode.QUIET.matches())
        self.assertFalse(demo.DefaultMode.LOUD.matches())
        self.assertTrue(demo.DefaultMode.LOUD.matches(mode=demo.DefaultMode.LOUD))
        self.assertEqual(demo.choose_default(), 0)
        self.assertEqual(demo.choose_default(second=demo.DefaultChoiceValue(4)), -4)
        self.assertEqual(demo.choose_default(demo.DefaultChoiceValue(5)), 5)
        self.assertEqual(demo.choose_default(second=None), 0)

    def test_numeric_limits_and_negative_zero(self) -> None:
        self.assertEqual(demo.default_float_bits(), 0x80000000)
        self.assertEqual(demo.default_double_bits(), 0x8000000000000000)
        self.assertEqual(demo.default_float_bits(0.0), 0)
        self.assertEqual(demo.default_double_bits(0.0), 0)
        limits = demo.DefaultedCounter.integer_limits()
        self.assertEqual(limits.lower, -(2**63))
        self.assertEqual(limits.upper, 2**64 - 1)

    def test_record_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_record_defaults")
        self.assertEqual(demo.DefaultAmount().value, 3)
        self.assertEqual(demo.DefaultAmount().offset(), 5)
        self.assertEqual(demo.DefaultAmount(value=5).offset(step=3), 8)
        self.assertEqual(demo.DefaultAmount.with_scaled_value().value, 4)
        self.assertEqual(demo.DefaultAmount.try_scaled_value().value, 4)
        self.assertIsNone(demo.DefaultAmount.try_scaled_value(value=-1))
        self.assertEqual(demo.NamedAmount.with_value().value, 5)
        self.assertEqual(demo.NamedAmount.with_value(value=9).value, 9)

    def test_custom_type_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_custom_type_defaults")
        self.assertEqual(demo.default_timeout_seconds(), 1.5)
        self.assertEqual(demo.default_timeout_seconds(demo.TimeoutFFI(seconds=2.5)), 2.5)
        self.assertIsNone(demo.default_limit())
        self.assertEqual(demo.default_limit(limit=7), 7)
        self.assertEqual(demo.default_email(), "mailto:ada@example.com")
        self.assertEqual(demo.default_email(email="mailto:grace@example.com"), "mailto:grace@example.com")
        self.assertEqual(demo.default_optional_email(), "mailto:ada@example.com")
        self.assertIsNone(demo.default_optional_email(email=None))

    def test_constructed_defaults_are_fresh_per_call(self) -> None:
        received = []
        encode_timeout = demo.TimeoutFFI._boltffi_wire

        def capture(timeout):
            received.append(timeout)
            return encode_timeout(timeout)

        with patch.object(demo.TimeoutFFI, "_boltffi_wire", capture):
            self.assertEqual(demo.default_timeout_seconds(), 1.5)
            self.assertEqual(demo.default_timeout_seconds(), 1.5)
        self.assertIsNot(received[0], received[1])

    def test_native_signature_exposes_literal_defaults(self) -> None:
        parameters = inspect.signature(demo.repeat_greeting).parameters
        self.assertIs(parameters["name"].default, inspect.Parameter.empty)
        self.assertEqual(parameters["greeting"].default, "hello")
        self.assertEqual(parameters["times"].default, 2)
        self.assertIs(parameters["shout"].default, False)

    def test_stubs_preserve_required_arguments_and_positional_calls(self) -> None:
        path = Path(demo.__file__).with_suffix(".pyi")
        source = path.read_text()
        compile(source, str(path), "exec")
        declarations = ast.parse(source)
        counter = next(node for node in declarations.body if isinstance(node, ast.ClassDef) and node.name == "DefaultedCounter")

        def signature(declaration):
            declaration.decorator_list = []
            namespace = {}
            exec(compile(ast.Module(body=[declaration], type_ignores=[]), str(path), "exec"), namespace)
            parameters = tuple(inspect.signature(namespace[declaration.name]).parameters.values())[1:]
            return inspect.Signature(parameters)

        signatures = [signature(node) for node in counter.body if isinstance(node, ast.FunctionDef) and node.name == "with_offset"]

        def accepts(*arguments, **keywords):
            def matches(signature):
                try:
                    signature.bind(*arguments, **keywords)
                    return True
                except TypeError:
                    return False

            return any(map(matches, signatures))

        self.assertTrue(accepts(5, 3))
        self.assertTrue(accepts(5, offset=3))
        self.assertTrue(accepts(offset=3))
        self.assertTrue(accepts(start=5, offset=3))
        self.assertFalse(accepts())
        self.assertFalse(accepts(5))
        self.assertFalse(accepts(start=5))


class AsyncDefaultArgumentsTests(AsyncDemoTestCase):
    async def test_async_defaults(self) -> None:
        self.demo_case("case:primitives.default_arguments.should_apply_async_defaults")
        self.assertEqual(await demo.async_default(), 9)
        self.assertEqual(await demo.async_default(value=5), 5)
        self.assertEqual(await demo.DefaultedCounter().async_offset(), 13)
        self.assertEqual(await demo.DefaultedCounter(5).async_offset(step=4), 9)
        counter = await demo.DefaultedCounter.start()
        self.assertEqual(counter.offset(), 31)
        counter = await demo.DefaultedCounter.start(second=DoublingCallback())
        self.assertEqual(counter.offset(), 61)
        counter = await demo.DefaultedCounter.start(5, DoublingCallback(), DoublingCallback())
        self.assertEqual(counter.offset(), 21)
        self.assertEqual((await demo.NamedAmount.load()).value, 6)
        self.assertEqual((await demo.NamedAmount.load(value=4)).value, 4)
        self.assertEqual(await demo.DefaultMode.load(), demo.DefaultMode.QUIET)
        self.assertEqual(await demo.DefaultMode.load(mode=demo.DefaultMode.LOUD), demo.DefaultMode.LOUD)
