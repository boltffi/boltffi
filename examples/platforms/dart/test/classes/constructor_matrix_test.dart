import 'dart:typed_data';

import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('constructor coverage matrix', () {
    final base = ConstructorCoverageMatrix();
    expect(base.constructorVariant(), 'new');
    expect(base.summary(), 'default');
    expect(base.payloadChecksum(), 0);
    expect(base.vectorCount(), 0);

    final scalarMix = ConstructorCoverageMatrix.withScalarMix(
      7,
      true,
      Priority.high,
    );
    expect(scalarMix.constructorVariant(), 'with_scalar_mix');
    expect(scalarMix.summary(), 'version=7;enabled=true;priority=high');
    expect(scalarMix.payloadChecksum(), 0);
    expect(scalarMix.vectorCount(), 0);

    final stringAndBytes = ConstructorCoverageMatrix.withStringAndBytes(
      'bolt',
      Uint8List.fromList([1, 2, 3, 4]),
    );
    expect(stringAndBytes.constructorVariant(), 'with_string_and_bytes');
    expect(stringAndBytes.summary(), 'label=bolt;bytes=4');
    expect(stringAndBytes.payloadChecksum(), 10);
    expect(stringAndBytes.vectorCount(), 4);

    final blittableAndRecord = ConstructorCoverageMatrix.withBlittableAndRecord(
      Point(x: 1.5, y: 2.5),
      Person(name: 'Alice', age: 31),
    );
    expect(
      blittableAndRecord.constructorVariant(),
      'with_blittable_and_record',
    );
    expect(blittableAndRecord.summary(), 'origin=1.5:2.5;person=Alice#31');
    expect(blittableAndRecord.payloadChecksum(), 0);
    expect(blittableAndRecord.vectorCount(), 1);

    final optionalProfileAndCursor =
        ConstructorCoverageMatrix.withOptionalProfileAndCursor(
          UserProfile(
            name: 'John',
            age: 29,
            email: 'john@example.com',
            score: 9.5,
          ),
          'cursor-7',
        );
    expect(
      optionalProfileAndCursor.constructorVariant(),
      'with_optional_profile_and_cursor',
    );
    expect(
      optionalProfileAndCursor.summary(),
      'profile=John#29#john@example.com#9.5;cursor=cursor-7',
    );
    expect(optionalProfileAndCursor.payloadChecksum(), 0);
    expect(optionalProfileAndCursor.vectorCount(), 2);

    final vectorsAndPolygon = ConstructorCoverageMatrix.withVectorsAndPolygon(
      ['ffi', 'dart'],
      [Point(x: 0.0, y: 0.0), Point(x: 1.0, y: 1.0)],
      Polygon(
        points: [
          Point(x: 0.0, y: 0.0),
          Point(x: 2.0, y: 0.0),
          Point(x: 1.0, y: 1.0),
        ],
      ),
    );
    expect(vectorsAndPolygon.constructorVariant(), 'with_vectors_and_polygon');
    expect(vectorsAndPolygon.summary(), 'tags=ffi|dart;anchors=2;polygon=3');
    expect(vectorsAndPolygon.payloadChecksum(), 0);
    expect(vectorsAndPolygon.vectorCount(), 7);

    final collectionRecords = ConstructorCoverageMatrix.withCollectionRecords(
      Team(name: 'Platform', members: ['Alice', 'John']),
      Classroom(
        students: [
          Person(name: 'Alice', age: 20),
          Person(name: 'John', age: 21),
        ],
      ),
      Polygon(
        points: [
          Point(x: 0.0, y: 0.0),
          Point(x: 1.0, y: 0.0),
          Point(x: 1.0, y: 1.0),
        ],
      ),
    );
    expect(collectionRecords.constructorVariant(), 'with_collection_records');
    expect(
      collectionRecords.summary(),
      'team=Platform;members=2;students=2;polygon=3',
    );
    expect(collectionRecords.payloadChecksum(), 0);
    expect(collectionRecords.vectorCount(), 7);

    final borrowedPoints = ConstructorCoverageMatrix.withBorrowedPoints(
      'borrowed',
      [Point(x: 2.0, y: 3.0), Point(x: 4.0, y: 5.0)],
    );
    expect(borrowedPoints.constructorVariant(), 'with_borrowed_points');
    expect(
      borrowedPoints.summary(),
      'label=borrowed;points=2;first=2.0:3.0',
      reason:
          "case:classes.constructor_matrix.with_borrowed_points.should_accept_borrowed_blittable_slice",
    );
    expect(borrowedPoints.payloadChecksum(), 0);
    expect(borrowedPoints.vectorCount(), 2);

    final borrowedPeople = ConstructorCoverageMatrix.withBorrowedPeople([
      Person(name: 'Ada', age: 40),
      Person(name: 'Grace', age: 41),
    ]);
    expect(borrowedPeople.constructorVariant(), 'with_borrowed_people');
    expect(
      borrowedPeople.summary(),
      'people=2;age_total=81;names=Ada|Grace',
      reason:
          "case:classes.constructor_matrix.with_borrowed_people.should_accept_borrowed_encoded_record_slice",
    );
    expect(borrowedPeople.payloadChecksum(), 0);
    expect(borrowedPeople.vectorCount(), 83);

    final enumMix = ConstructorCoverageMatrix.withEnumMix(
      Filter.byGroups(
        groups: [
          ['café', '🌍'],
          [],
          ['common'],
        ],
      ),
      Message.image(
        url: 'https://example.com/image.png',
        width: 640,
        height: 480,
      ),
      Task(title: 'ship', priority: Priority.critical, completed: false),
    );
    expect(enumMix.constructorVariant(), 'with_enum_mix');
    expect(
      enumMix.summary(),
      'filter=groups:3;message=image:https://example.com/image.png#640x480;task=ship#critical',
    );
    expect(enumMix.payloadChecksum(), 0);
    expect(enumMix.vectorCount(), 1);

    final everything = ConstructorCoverageMatrix.withEverything(
      Person(name: 'Alice', age: 31),
      Address(street: 'Main', city: 'AMS', zip: '1000'),
      UserProfile(name: 'John', age: 29, email: 'john@example.com', score: 9.5),
      SearchResult(
        query: 'route',
        total: 5,
        nextCursor: 'next-9',
        maxScore: 7.5,
      ),
      Uint8List.fromList([4, 5, 6]),
      Filter.byRange(min: 1.0, max: 3.0),
      ['alpha', 'beta'],
    );
    expect(everything.constructorVariant(), 'with_everything');
    expect(
      everything.summary(),
      'person=Alice#31;city=AMS;profile=profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0;tags=alpha|beta',
    );
    expect(everything.payloadChecksum(), 15);
    expect(everything.vectorCount(), 10);
    expect(
      everything.summarizeBorrowedInputs(
        UserProfile(
          name: 'John',
          age: 29,
          email: 'john@example.com',
          score: 9.5,
        ),
        SearchResult(
          query: 'route',
          total: 5,
          nextCursor: 'next-9',
          maxScore: 7.5,
        ),
        Filter.byRange(min: 1.0, max: 3.0),
      ),
      'profile=John#29#john@example.com#9.5;query=route;filter=range:1.0-3.0',
    );

    final fallible = ConstructorCoverageMatrix.tryWithPayloadAndSearchResult(
      Uint8List.fromList([7, 8]),
      SearchResult(
        query: 'search',
        total: 4,
        nextCursor: 'cursor-4',
        maxScore: null,
      ),
      Filter.byName(name: 'ali'),
    );
    expect(fallible.constructorVariant(), 'try_with_payload_and_search_result');
    expect(fallible.summary(), 'query=search;cursor=cursor-4;filter=name:ali');
    expect(fallible.payloadChecksum(), 15);
    expect(fallible.vectorCount(), 6);

    expect(
      () => ConstructorCoverageMatrix.tryWithPayloadAndSearchResult(
        Uint8List.fromList([]),
        SearchResult(
          query: 'search',
          total: 4,
          nextCursor: null,
          maxScore: null,
        ),
        Filter.none(),
      ),
      throwsBoltException("payload must not be empty"),
    );
  });
}
