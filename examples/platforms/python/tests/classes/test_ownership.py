import asyncio
import gc

import demo
from tests.support import AsyncDemoTestCase, DemoTestCase


class OwnershipTests(DemoTestCase):
    def test_transfer_disposal_and_preparation_failures(self):
        drops = demo.MessageDrops()
        message = demo.OwnedMessage("hello", drops)
        self.assertEqual(demo.consume_message(message), 5)
        self.assertIsNone(message._handle)
        del message
        gc.collect()
        self.assertEqual(drops.count(), 1)

        message = demo.OwnedMessage("", drops)
        with self.assertRaises(RuntimeError):
            demo.consume_message_result(message)
        self.assertIsNone(message._handle)
        self.assertEqual(drops.count(), 2)

        first = demo.OwnedMessage("first", drops)
        with self.assertRaises(ValueError):
            demo.consume_messages(first, message)
        self.assertIsNone(first._handle)
        self.assertEqual(drops.count(), 3)

        duplicate = demo.OwnedMessage("duplicate", drops)
        with self.assertRaises(ValueError):
            demo.consume_messages(duplicate, duplicate)
        self.assertIsNone(duplicate._handle)
        self.assertEqual(drops.count(), 4)

        receiver = demo.OwnedMessage("receiver", drops)
        with self.assertRaises(ValueError):
            receiver.combine(receiver)
        self.assertEqual(drops.count(), 5)

        unprepared = demo.OwnedMessage("unprepared", drops)
        with self.assertRaises(UnicodeEncodeError):
            demo.join_message(["\ud800"], unprepared)
        self.assertEqual(unprepared.length(), 10)
        self.assertEqual(drops.count(), 5)
        self.assertEqual(demo.consume_message_with_callback(lambda value: value + 1, unprepared), 11)
        self.assertEqual(drops.count(), 6)
        self.assertEqual(demo.consume_optional_message(None), 0)
        self.assertEqual(demo.consume_named_message(demo.OwnedMessage("named", drops), 1, 2, 3), 11)
        self.assertEqual(drops.count(), 7)


class AsyncOwnershipTests(AsyncDemoTestCase):
    async def test_rejected_transfer_during_active_borrow(self):
        drops = demo.MessageDrops()
        message = demo.OwnedMessage("retained", drops)
        pending = asyncio.create_task(demo.hold_message(message))
        async with asyncio.timeout(5):
            while drops.borrow_count() == 0:
                await asyncio.sleep(0)
        self.assertEqual(demo.consume_message(message), 0)
        self.assertIsNone(message._handle)
        del message
        gc.collect()
        self.assertEqual(drops.count(), 0)
        pending.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await pending
        self.assertEqual(drops.borrow_count(), 0)
        self.assertEqual(drops.count(), 1)

    async def test_async_store_result_and_nullable_transfer(self):
        drops = demo.MessageDrops()
        store = demo.MessageStore()
        message = demo.OwnedMessage("stored", drops)
        await store.set(message, {"trace": "context"})
        self.assertIsNone(message._handle)
        self.assertEqual(store.count(), 7)
        self.assertEqual(drops.count(), 1)
        self.assertEqual(await demo.consume_optional_message_async(None), 0)
        rejected = demo.OwnedMessage("", drops)
        with self.assertRaises(RuntimeError):
            await demo.consume_message_async(rejected)
        self.assertIsNone(rejected._handle)
        self.assertEqual(drops.count(), 2)
