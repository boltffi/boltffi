from tests.support import AsyncDemoTestCase

import demo


class AsyncFunctionTests(AsyncDemoTestCase):
    async def assert_runtime_error_contains(self, expected: str, call) -> None:
        with self.assertRaises(RuntimeError) as error:
            await call()
        self.assertIn(expected, str(error.exception))

    async def test_basic_async_functions(self) -> None:
        self.demo_case("case:async_fns.basic.add.should_return_sum")
        self.assertEqual(await demo.async_add(3, 7), 10)
        self.demo_case("case:async_fns.named_cancellation_token.should_preserve_both_values")
        self.assertEqual(await demo.async_cancellation_token_collision(7, 11), 18)
        self.demo_case("case:async_fns.basic.echo.should_prefix_message")
        self.assertEqual(await demo.async_echo("hello async"), "Echo: hello async")
        self.demo_case("case:async_fns.basic.double_all.should_double_i32_vector")
        self.assertEqual(await demo.async_double_all([1, 2, 3]), [2, 4, 6])
        self.demo_case("case:async_fns.basic.find_positive.should_return_first_positive")
        self.assertEqual(await demo.async_find_positive([-1, 0, 5, 3]), 5)
        self.demo_case("case:async_fns.basic.find_positive.should_return_none_for_all_negative")
        self.assertIsNone(await demo.async_find_positive([-1, -2, -3]))
        self.demo_case("case:async_fns.basic.concat.should_join_string_vector")
        self.assertEqual(await demo.async_concat(["a", "b", "c"]), "a, b, c")
        self.demo_case("case:async_fns.basic.get_numbers.should_return_counting_sequence")
        self.assertEqual(await demo.async_get_numbers(4), [0, 1, 2, 3])

    async def test_async_result_functions(self) -> None:
        self.demo_case("case:async_fns.results.fetch_data.should_return_scaled_positive_id")
        self.assertEqual(await demo.fetch_data(7), 70)
        self.demo_case("case:async_fns.results.fetch_data.should_reject_non_positive_id")
        await self.assert_runtime_error_contains("invalid id", lambda: demo.fetch_data(0))
        self.demo_case("case:async_fns.results.try_compute.should_return_doubled_value")
        self.assertEqual(await demo.try_compute_async(4), 8)
        self.demo_case("case:async_fns.results.try_compute.should_return_overflow_for_negative_value")
        await self.assert_typed_error_value(
            demo.ComputeErrorException,
            demo.ComputeErrorOverflow(-1, 0),
            lambda: demo.try_compute_async(-1),
        )
        self.demo_case("case:async_fns.results.try_compute.should_return_invalid_input_for_zero")
        await self.assert_typed_error_value(
            demo.ComputeErrorException,
            demo.ComputeErrorInvalidInput(-999),
            lambda: demo.try_compute_async(0),
        )

    async def test_async_mixed_record_functions(self) -> None:
        record = demo.MixedRecord(
            "sample",
            demo.Point(1.0, 2.0),
            demo.Priority.HIGH,
            demo.ShapeCircle(5.0),
            demo.MixedRecordParameters(
                ["alpha", "beta"],
                [demo.Point(0.0, 0.0), demo.Point(3.0, 4.0)],
                demo.Point(9.0, 9.0),
                3,
                False,
            ),
        )

        self.demo_case("case:async_fns.mixed_record.echo.should_roundtrip_record")
        self.assertEqual(await demo.async_echo_mixed_record(record), record)
        self.demo_case("case:async_fns.mixed_record.make.should_construct_record")
        self.assertEqual(
            await demo.async_make_mixed_record(
                record.name,
                record.anchor,
                record.priority,
                record.shape,
                record.parameters,
            ),
            record,
        )

    async def test_async_calls_resume_off_the_waking_thread(self) -> None:
        self.demo_case("case:async_fns.native_wake.resumed_thread.should_not_be_the_waking_thread")
        self.assertNotEqual(await demo.async_resumed_thread_name(), "boltffi-demo-waker")
