import demo

from tests.support import AsyncDemoTestCase


class Worker:
    def run(self, mode):
        if mode == 1:
            return False, demo.MathError.NEGATIVE_INPUT
        if mode == 2:
            raise ValueError("unchecked callback failure 東京🦀")
        return True, None

    def value(self, mode):
        success, payload = self.run(mode)
        return success, 42 if success else payload


class CallbackErrorTests(AsyncDemoTestCase):
    async def test_callback_errors_reach_rust(self):
        worker = Worker()
        self.demo_case("case:callbacks.errors.unit.should_report_success")
        self.assertEqual(demo.invoke_unit_worker(worker, 0), None)
        self.demo_case("case:callbacks.errors.unit.should_report_declared_error")
        with self.assertRaises(demo.MathErrorException) as error:
            demo.invoke_unit_worker(worker, 1)
        self.assertEqual(error.exception.error, demo.MathError.NEGATIVE_INPUT)
        self.demo_case("case:callbacks.errors.unit.should_report_unexpected_error")
        with self.assertRaises(demo.MathErrorException) as error:
            demo.invoke_unit_worker(worker, 2)
        self.assertEqual(error.exception.error, demo.MathError.OVERFLOW)
        self.demo_case("case:callbacks.errors.value.should_report_success")
        self.assertEqual(demo.invoke_value_worker(worker, 0), 42)
        self.demo_case("case:callbacks.errors.value.should_report_declared_error")
        with self.assertRaises(demo.MathErrorException) as error:
            demo.invoke_value_worker(worker, 1)
        self.assertEqual(error.exception.error, demo.MathError.NEGATIVE_INPUT)
        self.demo_case("case:callbacks.errors.value.should_report_unexpected_error")
        with self.assertRaises(demo.MathErrorException) as error:
            demo.invoke_value_worker(worker, 2)
        self.assertEqual(error.exception.error, demo.MathError.OVERFLOW)
        self.demo_case("case:callbacks.errors.async_unit.should_report_success")
        self.assertEqual(await demo.invoke_async_unit_worker(worker, 0), None)
        self.demo_case("case:callbacks.errors.async_unit.should_report_declared_error")
        with self.assertRaises(demo.MathErrorException) as error:
            await demo.invoke_async_unit_worker(worker, 1)
        self.assertEqual(error.exception.error, demo.MathError.NEGATIVE_INPUT)
        self.demo_case("case:callbacks.errors.async_unit.should_report_unexpected_error")
        with self.assertRaises(demo.MathErrorException) as error:
            await demo.invoke_async_unit_worker(worker, 2)
        self.assertEqual(error.exception.error, demo.MathError.OVERFLOW)
        self.demo_case("case:callbacks.errors.async_value.should_report_success")
        self.assertEqual(await demo.invoke_async_value_worker(worker, 0), 42)
        self.demo_case("case:callbacks.errors.async_value.should_report_declared_error")
        with self.assertRaises(demo.MathErrorException) as error:
            await demo.invoke_async_value_worker(worker, 1)
        self.assertEqual(error.exception.error, demo.MathError.NEGATIVE_INPUT)
        self.demo_case("case:callbacks.errors.async_value.should_report_unexpected_error")
        with self.assertRaises(demo.MathErrorException) as error:
            await demo.invoke_async_value_worker(worker, 2)
        self.assertEqual(error.exception.error, demo.MathError.OVERFLOW)

    def test_unexpected_closure_error_reaches_rust(self):
        def fail(value):
            raise ValueError("unexpected closure error")

        with self.assertRaises(demo.MathErrorException) as error:
            demo.apply_result_closure(fail, 0)
        self.assertEqual(error.exception.error, demo.MathError.OVERFLOW)
