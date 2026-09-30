from tests.support import DemoTestCase

import demo


class MethodsTests(DemoTestCase):
    def test_counter_method_from_a_methods_block(self) -> None:
        counter = demo.Counter(5)

        self.demo_case("case:classes.methods.counter.decrement.should_call_a_method_declared_in_a_methods_block")
        counter.decrement()
        self.assertEqual(counter.get(), 4)
