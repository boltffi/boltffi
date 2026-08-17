import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('async class methods', () async {
    final worker = AsyncWorker('test');
    try {
      expect(worker.getPrefix(), 'test');
      expect(await worker.process('data'), 'test: data');

      expect(await worker.findItem(42), 'test_42');
      expect(await worker.findItem(-1), isNull);

      final batch = await worker.processBatch(['x', 'y']);
      expect(batch.length, 2);
      expect(batch[0], 'test: x');
      expect(batch[1], 'test: y');
    } finally {
      worker.dispose$();
    }

    final service = MixedRecordService('records');
    try {
      final record = sampleMixedRecord();
      expect(service.getLabel(), 'records');
      expect(service.storedCount(), 0);
      expect(service.echoRecord(record), record);
      expect(
        service.storeRecordParts(
          record.name,
          record.anchor,
          record.priority,
          record.shape,
          record.parameters,
        ),
        record,
      );
      expect(service.storedCount(), 1);
      expect(await service.asyncEchoRecord(record), record);
      expect(
        await service.asyncStoreRecordParts(
          record.name,
          record.anchor,
          record.priority,
          record.shape,
          record.parameters,
        ),
        record,
      );
      expect(service.storedCount(), 2);
    } finally {
      service.dispose$();
    }
  });
}
