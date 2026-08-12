import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  test('data enums', () {
    final noneFilter = echoFilter(Filter.none());
    expect(
      noneFilter,
      isA<Filter$None>(),
      reason:
          "case:enums.complex_variants.filter.none.should_roundtrip_unit_variant",
    );

    final nameFilter = Filter.byName(name: 'alpha');
    expect(
      nameFilter,
      echoFilter(nameFilter),
      reason:
          "case:enums.complex_variants.filter.by_name.should_roundtrip_string_payload",
    );

    final tagsFilter = Filter.byTags(tags: ['a', 'b', 'c']);
    expect(
      echoFilter(tagsFilter),
      tagsFilter,
      reason:
          "case:enums.complex_variants.filter.by_tags.should_roundtrip_string_vector_payload",
    );

    final rangeFilter = Filter.byRange(min: 1.0, max: 10.0);
    expect(echoFilter(rangeFilter), rangeFilter);

    final groupFilter = Filter.byGroups(
      groups: [
        ['café', '🌍'],
        [],
        ['common'],
      ],
    );
    expect(
      echoFilter(groupFilter),
      groupFilter,
      reason:
          "case:enums.complex_variants.filter.by_groups.should_roundtrip_nested_string_vectors",
    );

    final anchors = [Point(x: 0.0, y: 0.0), Point(x: 1.0, y: 2.0)];
    final pointsFilter = Filter.byPoints(anchors: anchors);
    expect(
      pointsFilter,
      echoFilter(pointsFilter),
      reason:
          "case:enums.complex_variants.filter.by_points.should_roundtrip_record_vector_payload",
    );

    expect(
      describeFilter(nameFilter),
      'filter by name: alpha',
      reason:
          "case:enums.complex_variants.filter.by_name.should_describe_string_payload",
    );
    expect(
      describeFilter(rangeFilter),
      'filter by range: 1..10',
      reason:
          "case:enums.complex_variants.filter.by_range.should_describe_numeric_bounds",
    );
    expect(
      describeFilter(tagsFilter),
      'filter by 3 tags',
      reason:
          "case:enums.complex_variants.filter.by_tags.should_describe_string_vector_payload",
    );
    expect(
      describeFilter(groupFilter),
      'filter by 3 groups',
      reason:
          "case:enums.complex_variants.filter.by_groups.should_describe_nested_string_vectors",
    );
    expect(
      describeFilter(pointsFilter),
      'filter by 2 anchor points',
      reason:
          "case:enums.complex_variants.filter.by_points.should_describe_record_vector_payload",
    );

    final triangleHolder = makeTriangleHolder();
    expect(triangleHolder.shape, isA<Shape$Triangle>());

    final echoedHolder = echoHolder(triangleHolder);
    expect(echoedHolder, triangleHolder);

    final task = makeTask('Ship Java coverage', Priority.high);
    expect(task.title, 'Ship Java coverage');
    expect(task.priority, Priority.high);
    expect(task.completed, isFalse);

    final echoedTask = echoTask(task);
    expect(echoedTask, task);
    expect(
      isUrgent(
        Task(title: 'urgent', priority: Priority.critical, completed: false),
      ),
      isTrue,
    );
    expect(
      isUrgent(
        Task(title: 'urgent', priority: Priority.high, completed: false),
      ),
      isTrue,
    );
    expect(
      isUrgent(Task(title: 'calm', priority: Priority.low, completed: false)),
      isFalse,
    );
    expect(
      describeFilter(Filter.byPoints(anchors: anchors)),
      'filter by 2 anchor points',
    );

    final notification = Notification(
      message: 'hello',
      priority: Priority.medium,
      read: false,
    );
    final echoedNotification = echoNotification(notification);
    expect(echoedNotification, notification);

    final header = makeCriticalTaskHeader(42);
    expect(header.id, 42);
    expect(
      header.priority,
      Priority.critical,
      reason:
          "case:enums.data_enum.lifecycle_event.should_roundtrip_priority_payload",
    );
    expect(header.completed, isFalse);

    final echoedHeader = echoTaskHeader(header);
    expect(echoedHeader, header);

    final started = makeCriticalLifecycleEvent(7);
    expect(
      started,
      isA<LifecycleEvent$TaskStarted>(),
      reason: "case:enums.data_enum.lifecycle_event.should_make_critical_event",
    );
    final startedTs = started as LifecycleEvent$TaskStarted;
    expect(startedTs.priority, Priority.critical);
    expect(startedTs.id, 7);

    expect(echoLifecycleEvent(started), started);
    expect(
      echoLifecycleEvent(LifecycleEvent.tick()),
      isA<LifecycleEvent$Tick>(),
      reason:
          "case:enums.data_enum.lifecycle_event.should_roundtrip_tick_variant",
    );


    final circle = makeCircle(5.0);
    expect(
      circle,
      isA<Shape$Circle>(),
      reason:
          "case:enums.data_enum.shape.should_support_free_function_factories",
    );
    expect((circle as Shape$Circle).radius, 5.0);

    final rect = makeRectangle(3.0, 4.0);
    expect(rect, isA<Shape$Rectangle>());

    final staticNewShape = Shape.$new(6.0);
    expect(
      staticNewShape,
      isA<Shape$Circle>(),
      reason: "case:enums.data_enum.shape.should_support_primary_constructor",
    );
    expect((staticNewShape as Shape$Circle).radius, closeTo(6.0, 0.0001));

    final unitCircle = Shape.unitCircle();
    expect(
      unitCircle,
      isA<Shape$Circle>(),
      reason: "case:enums.data_enum.shape.unit_circle.should_construct_circle",
    );
    expect((unitCircle as Shape$Circle).radius, closeTo(1.0, 0.0001));

    final square = Shape.square(3.0);
    expect(
      square,
      isA<Shape$Rectangle>(),
      reason: "case:enums.data_enum.shape.square.should_construct_rectangle",
    );
    final sq = square as Shape$Rectangle;
    expect(sq.width, closeTo(3.0, 0.0001));
    expect(sq.height, closeTo(3.0, 0.0001));

    final checkedCircle = Shape.tryCircle(2.0);
    expect(
      checkedCircle,
      isA<Shape$Circle>(),
      reason:
          "case:enums.data_enum.shape.try_circle.should_return_circle_for_positive_radius",
    );

    expect(
      () => Shape.tryCircle(0.0),
      throwsBoltException("radius must be positive"),
      reason:
          "case:enums.data_enum.shape.should_reject_non_positive_circle_radius",
    );

    final yesMaybeCircle = Shape.maybeCircle(5.0);
    expect(
      yesMaybeCircle,
      isA<Shape$Circle>(),
      reason:
          "case:enums.data_enum.shape.maybe_circle.should_return_some_for_positive_radius",
    );

    final notMaybeCircle = Shape.maybeCircle(0.0);
    expect(
      notMaybeCircle,
      isNull,
      reason:
          "case:enums.data_enum.shape.maybe_circle.should_return_none_for_non_positive_radius",
    );

    expect(
      circle.area(),
      closeTo(3.141592653589793 * 25.0, 0.0001),
      reason:
          "case:enums.data_enum.shape.should_support_numeric_instance_methods",
    );
    expect(rect.area(), closeTo(12.0, 0.0001));

    expect(circle.describe(), 'circle r=5');
    expect(
      rect.describe(),
      'rect 3x4',
      reason:
          "case:enums.data_enum.shape.should_support_string_instance_methods",
    );

    expect(
      Shape.variantCount(),
      6,
      reason: "case:enums.data_enum.shape.should_report_variant_count",
    );

    final echoedCircle = echoShape(circle);
    expect(
      echoedCircle,
      isA<Shape$Circle>(),
      reason: "case:enums.data_enum.shape.should_roundtrip_core_variants",
    );
    expect((echoedCircle as Shape$Circle).radius, 5.0);

    final echoedRect = echoShape(rect);
    expect(
      echoedRect,
      isA<Shape$Rectangle>(),
      reason: "case:enums.data_enum.shape.should_roundtrip_core_variants",
    );

    final triangle = echoShape(
      Shape.triangle(
        a: Point(x: 0.0, y: 0.0),
        b: Point(x: 3.0, y: 0.0),
        c: Point(x: 0.0, y: 4.0),
      ),
    );
    expect(
      triangle,
      isA<Shape$Triangle>(),
      reason: "case:enums.data_enum.shape.should_roundtrip_core_variants",
    );

    final point = echoShape(Shape.point());
    expect(
      point,
      isA<Shape$Point>(),
      reason: "case:enums.data_enum.shape.should_roundtrip_core_variants",
    );

    final apexSome = echoShape(Shape.apex(tip: Point(x: 3.0, y: 4.0)));
    expect(
      apexSome,
      isA<Shape$Apex>(),
      reason:
          "case:enums.data_enum.shape.apex.should_roundtrip_some_point_payload",
    );
    expect((apexSome as Shape$Apex).tip, isNotNull);
    expect(apexSome.tip, Point(x: 3.0, y: 4.0));

    final apexNone = echoShape(Shape.apex(tip: null));
    expect(
      apexNone,
      isA<Shape$Apex>(),
      reason: "case:enums.data_enum.shape.apex.should_roundtrip_none_payload",
    );
    expect((apexNone as Shape$Apex).tip, isNull);

    final cluster = echoShape(Shape.cluster(members: [Point(x: 1.0, y: 2.0)]));
    expect(
      cluster,
      isA<Shape$Cluster>(),
      reason:
          "case:enums.data_enum.shape.should_roundtrip_vector_record_fields",
    );
    expect((cluster as Shape$Cluster).members.length, 1);
    expect(cluster.members[0], Point(x: 1.0, y: 2.0));

    final apexPoint = Shape.tryApexPoint(2.5);
    expect(
      apexPoint,
      Point(x: 0.0, y: 2.5),
      reason:
          "case:enums.data_enum.shape.try_apex_point.should_return_some_for_positive_radius",
    );
    expect(
      Shape.tryApexPoint(-1.0),
      isNull,
      reason:
          "case:enums.data_enum.shape.try_apex_point.should_return_none_for_non_positive_radius",
    );

    final text = echoMessage(Message.text(body: 'hello'));
    expect(
      text,
      isA<Message$Text>(),
      reason:
          "case:enums.data_enum.message.text.should_roundtrip_string_payload",
    );
    expect((text as Message$Text).body, 'hello');
    expect(
      messageSummary(Message.text(body: 'hi')),
      'text: hi',
      reason: "case:enums.data_enum.message.text.should_render_text_summary",
    );
    expect(
      messageSummary(Message.ping()),
      'ping',
      reason: "case:enums.data_enum.message.ping.should_render_ping_summary",
    );

    final dog = echoAnimal(Animal.dog(name: 'Rex', breed: 'Labrador'));
    expect(
      dog,
      isA<Animal$Dog>(),
      reason:
          "case:enums.data_enum.animal.dog.should_roundtrip_string_payloads",
    );
    expect((dog as Animal$Dog).name, 'Rex');
    expect(
      animalName(Animal.dog(name: 'Rex', breed: 'Labrador')),
      'Rex',
      reason: "case:enums.data_enum.animal.dog.should_derive_name",
    );

    final cat = echoAnimal(Animal.cat(name: 'Whiskers', indoor: true));
    expect(
      cat,
      isA<Animal$Cat>(),
      reason:
          "case:enums.data_enum.animal.cat.should_roundtrip_name_and_bool_payload",
    );
    expect((cat as Animal$Cat).name, 'Whiskers');
    expect(cat.indoor, isTrue);
    expect(
      animalName(Animal.cat(name: 'Whiskers', indoor: true)),
      'Whiskers',
      reason: "case:enums.data_enum.animal.cat.should_derive_name",
    );

    expect(
      animalName(Animal.fish(count: 5)),
      '5 fish',
      reason: "case:enums.data_enum.animal.fish.should_derive_count_label",
    );
    final fish = echoAnimal(Animal.fish(count: 7));
    expect(
      fish,
      isA<Animal$Fish>(),
      reason: "case:enums.data_enum.animal.fish.should_roundtrip_count_payload",
    );
    expect((fish as Animal$Fish).count, 7);

    final image = echoMessage(
      Message.image(
        url: 'https://example.com/cat.png',
        width: 640,
        height: 480,
      ),
    );
    expect(
      image,
      isA<Message$Image>(),
      reason:
          "case:enums.data_enum.message.image.should_roundtrip_url_dimensions_payload",
    );
    final img = image as Message$Image;
    expect(img.url, 'https://example.com/cat.png');
    expect(img.width, 640);
    expect(img.height, 480);
    expect(
      messageSummary(
        Message.image(
          url: 'https://example.com/cat.png',
          width: 640,
          height: 480,
        ),
      ),
      'image: 640x480 at https://example.com/cat.png',
      reason: "case:enums.data_enum.message.image.should_render_image_summary",
    );

    final ping = echoMessage(Message.ping());
    expect(
      ping,
      isA<Message$Ping>(),
      reason: "case:enums.data_enum.message.ping.should_roundtrip_unit_variant",
    );

    final redirect = echoApiResponse(
      ApiResponse.redirect(url: 'https://example.com/new'),
    );
    expect(
      redirect,
      isA<ApiResponse$Redirect>(),
      reason:
          "case:enums.complex_variants.api_response.redirect.should_roundtrip_url_payload",
    );
    expect((redirect as ApiResponse$Redirect).url, 'https://example.com/new');

    final success = echoApiResponse(ApiResponse.success(data: 'ok'));
    expect(
      success,
      isA<ApiResponse$Success>(),
      reason:
          "case:enums.complex_variants.api_response.success.should_roundtrip_string_payload",
    );
    expect(
      isSuccess(ApiResponse.success(data: 'data')),
      isTrue,
      reason: "case:enums.complex_variants.api_response.success.should_identify_success",
    );
    expect(
      isSuccess(ApiResponse.empty()),
      isFalse,
      reason:
          "case:enums.complex_variants.api_response.empty.should_not_identify_as_success",
    );

    final shapes = echoVecShape([
      Shape.circle(radius: 2.0),
      Shape.rectangle(width: 3.0, height: 4.0),
      Shape.point(),
    ]);
    expect(
      shapes.length,
      3,
      reason: "case:enums.data_enum.shape.should_roundtrip_vectors",
    );
    expect(shapes[0], isA<Shape$Circle>());
    expect((shapes[0] as Shape$Circle).radius, closeTo(2.0, 0.0001));
    expect(shapes[1], isA<Shape$Rectangle>());
    expect(shapes[2], isA<Shape$Point>());
  });
}
