import gc
from unittest.mock import patch

import demo
from tests.support import DemoTestCase


class Receiver:
    def __init__(self):
        self.first = None
        self.second = None
        self.fail = False

    def attach(self, handle, callback):
        if callback != 42:
            raise AssertionError(callback)
        self.first = handle

    def optional(self, handle):
        self.first = handle

    def pair(self, first, label, second):
        if label != "pair":
            raise AssertionError(label)
        self.first = first
        self.second = second
        if self.fail:
            return False, demo.MathError.NEGATIVE_INPUT
        return True, first.length() + (second.length() if second is not None else 0)


class ClassHandleTests(DemoTestCase):
    def test_retained_message_outlives_callback(self):
        self.demo_case("case:callbacks.class_handles.should_retain_after_return")
        drops = demo.MessageDrops()
        receiver = Receiver()
        demo.deliver_message(receiver, drops)
        self.assertEqual(drops.count(), 0)
        self.assertEqual(receiver.first.length(), 9)
        receiver.first = None
        gc.collect()
        self.assertEqual(drops.count(), 1)

    def test_multiple_and_optional_messages(self):
        self.demo_case("case:callbacks.class_handles.should_deliver_multiple_and_optional")
        drops = demo.MessageDrops()
        receiver = Receiver()
        self.assertEqual(demo.deliver_message_pair(receiver, drops, True), 11)
        self.assertEqual(drops.count(), 0)
        self.assertEqual(receiver.first.length(), 5)
        self.assertEqual(receiver.second.length(), 6)
        receiver.first = None
        self.assertEqual(drops.count(), 1)
        receiver.second = None
        self.assertEqual(drops.count(), 2)
        self.assertEqual(demo.deliver_message_pair(receiver, drops, False), 5)
        self.assertIsNone(receiver.second)
        self.assertEqual(drops.count(), 2)
        receiver.first = None
        self.assertEqual(drops.count(), 3)

    def test_stored_messages_survive_callback_error(self):
        self.demo_case("case:callbacks.class_handles.should_retain_after_error")
        drops = demo.MessageDrops()
        receiver = Receiver()
        receiver.fail = True
        with self.assertRaises(demo.MathErrorException) as error:
            demo.deliver_message_pair(receiver, drops, True)
        self.assertEqual(error.exception.error, demo.MathError.NEGATIVE_INPUT)
        self.assertEqual(drops.count(), 0)
        self.assertEqual(receiver.first.length(), 5)
        self.assertEqual(receiver.second.length(), 6)
        receiver.first = None
        receiver.second = None
        self.assertEqual(drops.count(), 2)

    def test_missing_callback_method_releases_undelivered_messages(self):
        drops = demo.MessageDrops()
        with self.assertRaises(demo.MathErrorException):
            demo.deliver_message_pair(object(), drops, True)
        self.assertEqual(drops.count(), 2)

    def test_second_wrapper_failure_releases_both_undelivered_messages(self):
        drops = demo.MessageDrops()
        receiver = Receiver()
        wrapped = 0

        def fail_second_wrapper(instance, name, value):
            nonlocal wrapped
            if name == "_handle" and value is not None:
                wrapped += 1
                if wrapped == 2:
                    raise MemoryError("second wrapper failed")
            object.__setattr__(instance, name, value)

        with patch.object(demo.OwnedMessage, "__setattr__", fail_second_wrapper):
            with self.assertRaises(demo.MathErrorException):
                demo.deliver_message_pair(receiver, drops, True)
        self.assertEqual(wrapped, 2)
        self.assertIsNone(receiver.first)
        self.assertIsNone(receiver.second)
        self.assertEqual(drops.count(), 2)
