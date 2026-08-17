import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('single-threaded state holder', () async {
    final holder = StateHolder('local');
    try {
      final doubler = DoublerValueCallbackImpl();

      expect(holder.getLabel(), 'local');
      expect(holder.getValue(), 0);
      holder.setValue(5);
      expect(holder.getValue(), 5);
      expect(holder.increment(), 6);
      holder.addItem('a');
      holder.addItem('b');
      expect(holder.itemCount(), 2);
      expect(holder.getItems(), ['a', 'b']);
      expect(holder.removeLast(), 'b');
      expect(holder.transformValue((value) => value ~/ 2), 3);
      expect(holder.applyValueCallback(doubler), 6);
      expect(await holder.asyncGetValue(), 6);
      await holder.asyncSetValue(9);
      expect(holder.getValue(), 9);
      expect(await holder.asyncAddItem('z'), 2);
      expect(holder.getItems(), ['a', 'z']);
      holder.clear();
      expect(holder.getValue(), 0);
      expect(holder.getItems(), isEmpty);
    } finally {
      holder.dispose$();
    }

    final mapView = MapView();
    try {
      final marker = mapView.addMarker(MarkerOptions(id: 7, title: 'harbor'));
      expect(marker.id(), 7);
      expect(
        marker.title(),
        'harbor',
        reason:
            "case:classes.unsafe_single_threaded.map_view.add_marker.should_return_single_threaded_marker_handle",
      );
      expect(mapView.markerCount(), 1);
    } finally {
      mapView.dispose$();
    }
  });
}
