#include <math.h>
#include <string.h>

#include "test.h"

bool test_owned_records(void) {
    const DemoPersonView person = {{"Ali", 3}, 32};
    DemoPerson returned_person = demo_echo_person(person);
    CHECK(returned_person.age == 32 && returned_person.name.len == 3 && memcmp(returned_person.name.ptr, "Ali", 3) == 0, "case:records.with_strings.person.should_roundtrip_value");
    demo_person_free(&returned_person);
    returned_person = demo_make_person(person.name, 32);
    CHECK(returned_person.age == 32 && returned_person.name.len == 3 && memcmp(returned_person.name.ptr, "Ali", 3) == 0, "case:records.with_strings.person.should_make_from_fields");
    demo_person_free(&returned_person);
    CHECK_STRING(demo_greet_person(person), "Hello, Ali! You are 32 years old.", "case:records.with_strings.person.should_format_greeting");
    const DemoAddressView address = {{"Main St", 7}, {"Utrecht", 7}, {"3511", 4}};
    DemoAddress returned_address = demo_echo_address(address);
    CHECK(returned_address.street.len == 7 && memcmp(returned_address.street.ptr, "Main St", 7) == 0 && returned_address.city.len == 7 && memcmp(returned_address.city.ptr, "Utrecht", 7) == 0 && returned_address.zip.len == 4 && memcmp(returned_address.zip.ptr, "3511", 4) == 0, "case:records.with_strings.address.should_roundtrip_value");
    demo_address_free(&returned_address);
    CHECK_STRING(demo_format_address(address), "Main St, Utrecht, 3511", "case:records.with_strings.address.should_format_value");
    const DemoUserProfileView profile = {{"Ali", 3}, 32, {true, {"ali@example.com", 15}}, {true, 9.5}};
    DemoUserProfile returned_profile = demo_echo_user_profile(profile);
    CHECK(returned_profile.name.len == 3 && memcmp(returned_profile.name.ptr, "Ali", 3) == 0 && returned_profile.age == 32 && returned_profile.email.has_value && returned_profile.email.value.len == 15 && memcmp(returned_profile.email.value.ptr, "ali@example.com", 15) == 0 && returned_profile.score.has_value && returned_profile.score.value == 9.5, "case:records.with_options.user_profile.should_roundtrip_present_options");
    demo_user_profile_free(&returned_profile);
    const DemoUserProfileView absent = {{"Ali", 3}, 32, {false, {0}}, {false, 0}};
    returned_profile = demo_echo_user_profile(absent);
    CHECK(returned_profile.age == 32 && !returned_profile.email.has_value && !returned_profile.score.has_value, "case:records.with_options.user_profile.should_roundtrip_absent_options");
    demo_user_profile_free(&returned_profile);
    returned_profile = demo_echo_user_profile((DemoUserProfileView){{"Ali", 3}, 32, {false, {0}}, {true, 9.5}});
    CHECK(!returned_profile.email.has_value && returned_profile.score.has_value && returned_profile.score.value == 9.5, "case:records.with_options.user_profile.should_roundtrip_mixed_options");
    demo_user_profile_free(&returned_profile);
    const char utf8_email[] = "caf\xc3\xa9@example.com";
    returned_profile = demo_echo_user_profile((DemoUserProfileView){{"Ali", 3}, 32, {true, {utf8_email, sizeof(utf8_email) - 1}}, {false, 0}});
    CHECK(returned_profile.email.has_value && returned_profile.email.value.len == sizeof(utf8_email) - 1 && memcmp(returned_profile.email.value.ptr, utf8_email, sizeof(utf8_email) - 1) == 0, "case:records.with_options.user_profile.should_roundtrip_utf8_optional_string");
    demo_user_profile_free(&returned_profile);
    returned_profile = demo_make_user_profile(profile.name, profile.age, profile.email, profile.score);
    CHECK(returned_profile.age == 32 && returned_profile.email.has_value && returned_profile.email.value.len == 15 && returned_profile.score.has_value && returned_profile.score.value == 9.5, "case:records.with_options.user_profile.should_make_with_present_options");
    demo_user_profile_free(&returned_profile);
    returned_profile = demo_make_user_profile(absent.name, absent.age, absent.email, absent.score);
    CHECK(returned_profile.age == 32 && !returned_profile.email.has_value && !returned_profile.score.has_value, "case:records.with_options.user_profile.should_make_with_absent_options");
    demo_user_profile_free(&returned_profile);
    CHECK_STRING(demo_user_display_name(profile), "Ali <ali@example.com>", "case:records.with_options.user_profile.should_display_email_when_present");
    CHECK_STRING(demo_user_display_name(absent), "Ali", "case:records.with_options.user_profile.should_display_name_when_email_absent");
    const DemoSearchResultView search = {{"query", 5}, 42, {true, {"cursor", 6}}, {true, 0.75}};
    DemoSearchResult returned_search = demo_echo_search_result(search);
    CHECK(returned_search.query.len == 5 && memcmp(returned_search.query.ptr, "query", 5) == 0 && returned_search.total == 42 && returned_search.next_cursor.has_value && returned_search.next_cursor.value.len == 6 && memcmp(returned_search.next_cursor.value.ptr, "cursor", 6) == 0 && returned_search.max_score.has_value && returned_search.max_score.value == 0.75, "case:records.with_options.search_result.should_roundtrip_present_options");
    demo_search_result_free(&returned_search);
    const DemoSearchResultView exhausted = {{"query", 5}, 42, {false, {0}}, {false, 0}};
    returned_search = demo_echo_search_result(exhausted);
    CHECK(returned_search.total == 42 && !returned_search.next_cursor.has_value && !returned_search.max_score.has_value, "case:records.with_options.search_result.should_roundtrip_absent_options");
    demo_search_result_free(&returned_search);
    CHECK(demo_has_more_results(search), "case:records.with_options.search_result.should_report_more_results_when_cursor_present");
    CHECK(!demo_has_more_results(exhausted), "case:records.with_options.search_result.should_report_no_more_results_without_cursor");
    return true;
}

bool test_record_collections(void) {
    const DemoPoint points[] = {{1, 2}, {3, 4}};
    const DemoPolygonView polygon = {{points, 2}};
    DemoPolygon returned_polygon = demo_echo_polygon(polygon);
    CHECK(returned_polygon.points.len == 2 && returned_polygon.points.ptr[0].x == 1 && returned_polygon.points.ptr[0].y == 2 && returned_polygon.points.ptr[1].x == 3 && returned_polygon.points.ptr[1].y == 4, "case:records.with_collections.polygon.should_roundtrip_point_vector");
    demo_polygon_free(&returned_polygon);
    returned_polygon = demo_make_polygon(polygon.points);
    CHECK(returned_polygon.points.len == 2 && returned_polygon.points.ptr[0].x == 1 && returned_polygon.points.ptr[1].y == 4, "case:records.with_collections.polygon.should_make_from_points");
    demo_polygon_free(&returned_polygon);
    CHECK(demo_polygon_vertex_count(polygon) == 2, "case:records.with_collections.polygon.should_report_vertex_count");
    DemoPoint centroid = demo_polygon_centroid(polygon);
    CHECK(centroid.x == 2 && centroid.y == 3, "case:records.with_collections.polygon.should_compute_centroid");
    const DemoStringView members[] = {{"Ali", 3}, {"Sam", 3}};
    const DemoTeamView team = {{"team", 4}, {members, 2}};
    DemoTeam returned_team = demo_echo_team(team);
    CHECK(returned_team.name.len == 4 && memcmp(returned_team.name.ptr, "team", 4) == 0 && returned_team.members.len == 2 && returned_team.members.ptr[0].len == 3 && memcmp(returned_team.members.ptr[0].ptr, "Ali", 3) == 0 && returned_team.members.ptr[1].len == 3 && memcmp(returned_team.members.ptr[1].ptr, "Sam", 3) == 0, "case:records.with_collections.team.should_roundtrip_member_vector");
    demo_team_free(&returned_team);
    returned_team = demo_make_team(team.name, team.members);
    CHECK(returned_team.name.len == 4 && returned_team.members.len == 2 && returned_team.members.ptr[1].len == 3 && memcmp(returned_team.members.ptr[1].ptr, "Sam", 3) == 0, "case:records.with_collections.team.should_make_from_members");
    demo_team_free(&returned_team);
    CHECK(demo_team_size(team) == 2, "case:records.with_collections.team.should_report_member_count");
    const DemoPersonView students[] = {{{"Ali", 3}, 32}, {{"Sam", 3}, 28}};
    const DemoClassroomView classroom = {{students, 2}};
    DemoClassroom returned_classroom = demo_echo_classroom(classroom);
    CHECK(returned_classroom.students.len == 2 && returned_classroom.students.ptr[0].age == 32 && returned_classroom.students.ptr[0].name.len == 3 && memcmp(returned_classroom.students.ptr[0].name.ptr, "Ali", 3) == 0 && returned_classroom.students.ptr[1].age == 28 && returned_classroom.students.ptr[1].name.len == 3 && memcmp(returned_classroom.students.ptr[1].name.ptr, "Sam", 3) == 0, "case:records.with_collections.classroom.should_roundtrip_student_vector");
    demo_classroom_free(&returned_classroom);
    returned_classroom = demo_make_classroom(classroom.students);
    CHECK(returned_classroom.students.len == 2 && returned_classroom.students.ptr[0].age == 32 && returned_classroom.students.ptr[1].age == 28, "case:records.with_collections.classroom.should_make_from_students");
    demo_classroom_free(&returned_classroom);
    const double scores[] = {1.5, 2.5, 3.5};
    const DemoTaggedScoresView tagged = {{"scores", 6}, {scores, 3}};
    DemoTaggedScores returned_scores = demo_echo_tagged_scores(tagged);
    CHECK(returned_scores.label.len == 6 && memcmp(returned_scores.label.ptr, "scores", 6) == 0 && returned_scores.scores.len == 3 && memcmp(returned_scores.scores.ptr, scores, sizeof(scores)) == 0, "case:records.with_collections.tagged_scores.should_roundtrip_score_vector");
    demo_tagged_scores_free(&returned_scores);
    CHECK(fabs(demo_average_score(tagged) - 2.5) < 1e-12, "case:records.with_collections.tagged_scores.should_average_scores");
    DemoBenchmarkUserProfileSequence generated = demo_generate_user_profiles(3);
    CHECK(generated.len == 3 && generated.ptr[2].id == 2 && generated.ptr[2].name.len == 6 && memcmp(generated.ptr[2].name.ptr, "User 2", 6) == 0 && generated.ptr[2].email.len == 17 && memcmp(generated.ptr[2].email.ptr, "user2@example.com", 17) == 0 && generated.ptr[2].age == 22 && generated.ptr[2].score == 3.0 && generated.ptr[2].tags.len == 3 && generated.ptr[2].scores.len == 3 && generated.ptr[2].scores.ptr[2] == 22 && generated.ptr[2].is_active && !generated.ptr[1].is_active, "case:records.with_collections.user_profiles.should_generate_profiles");
    demo_benchmark_user_profile_sequence_free(&generated);
    const DemoBenchmarkUserProfileView profiles[] = {
        {.id = 1, .name = {"Ali", 3}, .email = {"ali@example.com", 15}, .score = 1.5, .is_active = true},
        {.id = 2, .name = {"Sam", 3}, .email = {"sam@example.com", 15}, .score = 2.5, .is_active = false}
    };
    CHECK(demo_sum_user_scores((DemoBenchmarkUserProfileSlice){profiles, 2}) == 4.0, "case:records.with_collections.user_profiles.should_sum_scores");
    CHECK(demo_count_active_users((DemoBenchmarkUserProfileSlice){profiles, 2}) == 1, "case:records.with_collections.user_profiles.should_count_active_users");
    return true;
}

bool test_nested_records(void) {
    const DemoLine line = {{1, 2}, {4, 6}};
    DemoLine returned_line = demo_echo_line(line);
    CHECK(returned_line.start.x == 1 && returned_line.start.y == 2 && returned_line.end.x == 4 && returned_line.end.y == 6, "case:records.nested.line.should_roundtrip_nested_points");
    returned_line = demo_make_line(1, 2, 4, 6);
    CHECK(returned_line.start.x == 1 && returned_line.start.y == 2 && returned_line.end.x == 4 && returned_line.end.y == 6, "case:records.nested.line.should_make_from_coordinates");
    CHECK(demo_line_length(line) == 5, "case:records.nested.line.should_compute_length");
    const DemoRect rectangle = {{1, 2}, {3, 4}};
    DemoRect returned_rectangle = demo_echo_rect(rectangle);
    CHECK(returned_rectangle.origin.x == 1 && returned_rectangle.origin.y == 2 && returned_rectangle.dimensions.width == 3 && returned_rectangle.dimensions.height == 4, "case:records.nested.rect.should_roundtrip_nested_records");
    CHECK(demo_rect_area(rectangle) == 12, "case:records.nested.rect.should_compute_area");
    const DemoTaskView task = {{"task", 4}, DEMO_PRIORITY_HIGH, true};
    DemoTask returned_task = demo_echo_task(task);
    CHECK(returned_task.title.len == 4 && memcmp(returned_task.title.ptr, "task", 4) == 0 && returned_task.priority == DEMO_PRIORITY_HIGH && returned_task.completed, "case:records.with_enums.task.should_roundtrip_priority_field");
    demo_task_free(&returned_task);
    returned_task = demo_make_task(task.title, task.priority);
    CHECK(returned_task.title.len == 4 && returned_task.priority == DEMO_PRIORITY_HIGH && !returned_task.completed, "case:records.with_enums.task.should_make_incomplete_task");
    demo_task_free(&returned_task);
    CHECK(demo_is_urgent(task) && !demo_is_urgent((DemoTaskView){{"low", 3}, DEMO_PRIORITY_LOW, false}), "case:records.with_enums.task.should_detect_urgent_priority");
    DemoNotification notification = demo_echo_notification((DemoNotificationView){{"hello", 5}, DEMO_PRIORITY_CRITICAL, true});
    CHECK(notification.message.len == 5 && memcmp(notification.message.ptr, "hello", 5) == 0 && notification.priority == DEMO_PRIORITY_CRITICAL && notification.read, "case:records.with_enums.notification.should_roundtrip_priority_field");
    demo_notification_free(&notification);
    DemoHolder holder = demo_echo_holder((DemoHolderView){{.tag = DEMO_SHAPE_CIRCLE, .data.circle = {2.5}}});
    CHECK(holder.shape.tag == DEMO_SHAPE_CIRCLE && holder.shape.data.circle.radius == 2.5, "case:records.with_enums.holder.should_roundtrip_data_enum_field");
    demo_holder_free(&holder);
    holder = demo_make_triangle_holder();
    CHECK(holder.shape.tag == DEMO_SHAPE_TRIANGLE && holder.shape.data.triangle.a.x == 0 && holder.shape.data.triangle.b.x == 4 && holder.shape.data.triangle.c.y == 3, "case:records.with_enums.holder.should_make_triangle_variant");
    demo_holder_free(&holder);
    DemoTaskHeader header = demo_echo_task_header((DemoTaskHeader){42, DEMO_PRIORITY_HIGH, true});
    CHECK(header.id == 42 && header.priority == DEMO_PRIORITY_HIGH && header.completed, "case:records.with_enums.task_header.should_roundtrip_repr_enum_field");
    header = demo_make_critical_task_header(7);
    CHECK(header.id == 7 && header.priority == DEMO_PRIORITY_CRITICAL && !header.completed, "case:records.with_enums.task_header.should_make_critical_header");
    DemoLogEntry log = demo_echo_log_entry((DemoLogEntry){-123456, DEMO_LOG_LEVEL_WARN, 65535});
    CHECK(log.timestamp == -123456 && log.level == DEMO_LOG_LEVEL_WARN && log.code == 65535, "case:records.with_enums.log_entry.should_roundtrip_u8_enum_field");
    log = demo_make_error_log_entry(123456, 42);
    CHECK(log.timestamp == 123456 && log.level == DEMO_LOG_LEVEL_ERROR && log.code == 42, "case:records.with_enums.log_entry.should_make_error_entry");
    const DemoStringView tags[] = {{"tag", 3}};
    const DemoPoint points[] = {{1, 2}, {3, 4}};
    const DemoMixedRecordView mixed = {
        .name = {"mixed", 5}, .anchor = {2, 3}, .priority = DEMO_PRIORITY_HIGH,
        .shape = {.tag = DEMO_SHAPE_CLUSTER, .data.cluster = {{points, 2}}},
        .parameters = {{tags, 1}, {points, 2}, {true, {5, 6}}, 7, true}
    };
    DemoMixedRecord returned = demo_echo_mixed_record(mixed);
    CHECK(returned.name.len == 5 && memcmp(returned.name.ptr, "mixed", 5) == 0 && returned.anchor.x == 2 && returned.anchor.y == 3 && returned.priority == DEMO_PRIORITY_HIGH && returned.shape.tag == DEMO_SHAPE_CLUSTER && returned.shape.data.cluster.members.len == 2 && returned.shape.data.cluster.members.ptr[1].y == 4 && returned.parameters.tags.len == 1 && returned.parameters.tags.ptr[0].len == 3 && memcmp(returned.parameters.tags.ptr[0].ptr, "tag", 3) == 0 && returned.parameters.checkpoints.len == 2 && returned.parameters.checkpoints.ptr[1].x == 3 && returned.parameters.fallback_anchor.has_value && returned.parameters.fallback_anchor.value.y == 6 && returned.parameters.max_retries == 7 && returned.parameters.preview_only, "case:records.mixed.should_roundtrip_composed_record");
    demo_mixed_record_free(&returned);
    returned = demo_make_mixed_record(mixed.name, mixed.anchor, mixed.priority, mixed.shape, mixed.parameters);
    CHECK(returned.name.len == 5 && memcmp(returned.name.ptr, "mixed", 5) == 0 && returned.anchor.x == 2 && returned.priority == DEMO_PRIORITY_HIGH && returned.shape.tag == DEMO_SHAPE_CLUSTER && returned.shape.data.cluster.members.len == 2 && returned.parameters.tags.len == 1 && returned.parameters.checkpoints.len == 2 && returned.parameters.fallback_anchor.has_value && returned.parameters.max_retries == 7 && returned.parameters.preview_only, "case:records.mixed.should_make_from_composed_parts");
    demo_mixed_record_free(&returned);
    return true;
}

bool test_service_configs(void) {
    DemoRequestConfig request_config = demo_request_config_init();
    CHECK(demo_request_timeout_seconds(request_config) == 1.5, "case:records.default_values.custom_type.should_apply_default");
    DemoServiceConfigView defaults = demo_service_config_init(demo_string_view("worker", 6));
    CHECK(defaults.name.len == 6 && defaults.retries == 3, "record initializer preserves required arguments and numeric defaults");
    CHECK(defaults.region.len == 8 && memcmp(defaults.region.ptr, "standard", 8) == 0, "record initializer uses borrowed string defaults");
    CHECK(!defaults.endpoint.has_value && defaults.backup_endpoint.has_value, "record initializer applies optional defaults");
    CHECK(defaults.backup_endpoint.value.len == 15, "record initializer retains default string length");

    const DemoStringView name = {"service", 7};
    DemoServiceConfig config = demo_service_config_from_owned_name(name);
    CHECK(config.name.len == 7 && memcmp(config.name.ptr, "service", 7) == 0 && config.retries == 3 && config.region.len == 8 && !config.endpoint.has_value && config.backup_endpoint.has_value, "case:records.default_values.service_config.from_owned_name.should_return_config");
    demo_service_config_free(&config);
    config = demo_service_config_from_borrowed_name(name);
    CHECK(config.name.len == 7 && memcmp(config.name.ptr, "service", 7) == 0 && config.retries == 3, "case:records.default_values.service_config.from_borrowed_name.should_return_config");
    demo_service_config_free(&config);
    config = demo_service_config_from_string_ref_name(name);
    CHECK(config.name.len == 7 && memcmp(config.name.ptr, "service", 7) == 0 && config.retries == 3, "case:records.default_values.service_config.from_string_ref_name.should_return_config");
    demo_service_config_free(&config);
    DemoOptionServiceConfig optional = demo_service_config_from_non_empty_name(name, (DemoStringView){"eu", 2});
    CHECK(optional.has_value && optional.value.name.len == 7 && optional.value.region.len == 2 && memcmp(optional.value.region.ptr, "eu", 2) == 0, "case:records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values");
    demo_option_service_config_free(&optional);
    optional = demo_service_config_from_non_empty_name((DemoStringView){NULL, 0}, (DemoStringView){"eu", 2});
    CHECK(!optional.has_value, "case:records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values");
    demo_option_service_config_free(&optional);
    const DemoServiceConfigView view = {name, 3, {"standard", 8}, {false, {0}}, {true, {"https://default", 15}}};
    CHECK_STRING(demo_service_config_describe(&view), "service:3:standard:none:https://default", "case:records.default_values.service_config.should_describe_values");
    CHECK_STRING(demo_service_config_describe_with_prefix(&view, (DemoStringView){"prefix", 6}), "prefix:service:3:standard:none:https://default", "case:records.default_values.service_config.should_describe_with_prefix");
    DemoServiceConfigTryWithRetriesResult tried = demo_service_config_try_with_retries(7);
    CHECK(tried.ok && tried.data.value.retries == 7 && tried.data.value.name.len == 9 && memcmp(tried.data.value.name.ptr, "generated", 9) == 0, "case:records.default_values.service_config.try_with_retries.should_return_config");
    demo_service_config_try_with_retries_result_free(&tried);
    tried = demo_service_config_try_with_retries(-1);
    const char expected_error[] = "service config retries must be non-negative";
    CHECK(!tried.ok && tried.data.error.len == sizeof(expected_error) - 1 && memcmp(tried.data.error.ptr, expected_error, tried.data.error.len) == 0, "case:records.default_values.service_config.try_with_retries.should_reject_negative_retries");
    demo_service_config_try_with_retries_result_free(&tried);
    optional = demo_service_config_maybe_with_retries(4);
    CHECK(optional.has_value && optional.value.retries == 4, "case:records.default_values.service_config.maybe_with_retries.should_return_some");
    demo_option_service_config_free(&optional);
    optional = demo_service_config_maybe_with_retries(-1);
    CHECK(!optional.has_value, "case:records.default_values.service_config.maybe_with_retries.should_return_none");
    demo_option_service_config_free(&optional);
    config = demo_echo_service_config(view);
    CHECK(config.name.len == 7 && memcmp(config.name.ptr, name.ptr, 7) == 0 && config.retries == 3 && config.region.len == 8 && memcmp(config.region.ptr, "standard", 8) == 0 && !config.endpoint.has_value && config.backup_endpoint.has_value && config.backup_endpoint.value.len == 15 && memcmp(config.backup_endpoint.value.ptr, "https://default", 15) == 0, "case:records.default_values.service_config.should_roundtrip_value");
    demo_service_config_free(&config);
    return true;
}

bool test_mutable_values(void) {
    DemoMutableMode mode = DEMO_MUTABLE_MODE_IDLE;
    demo_mutable_mode_start(&mode);
    CHECK(mode == DEMO_MUTABLE_MODE_RUNNING, "mutable scalar enum writes back its new variant");
    DemoMutableRecord record = demo_mutable_record_new(demo_string_view("original", 8));
    CHECK(demo_mutable_record_rename(&record, demo_string_view("updated", 7)) == 2, "mutable encoded receiver returns its result");
    CHECK(record.title.len == 7 && memcmp(record.title.ptr, "updated", 7) == 0, "mutable encoded receiver replaces its string");
    CHECK(record.tags.len == 2 && record.tags.ptr[1].len == 7, "mutable encoded receiver writes back nested strings");
    demo_mutable_record_free(&record);
    DemoString text = demo_echo_string(demo_string_view("original", 8));
    demo_replace_mutable_text(&text);
    CHECK(text.len == 7 && memcmp(text.ptr, "updated", 7) == 0, "mutable string parameter is replaced");
    demo_string_free(&text);
    DemoMutableRecordSequence records = {0};
    demo_replace_mutable_records(&records);
    CHECK(records.len == 1 && records.ptr[0].title.len == 7, "mutable record vector grows from empty");
    demo_replace_mutable_records(&records);
    CHECK(records.len == 1 && records.ptr[0].tags.len == 1, "mutable record vector releases previous contents");
    demo_mutable_record_sequence_free(&records);
    DemoMutableMessage message = demo_mutable_message_text(demo_string_view("original", 8));
    demo_mutable_message_replace(&message);
    CHECK(message.tag == DEMO_MUTABLE_MESSAGE_RECORD, "mutable data enum changes variants");
    CHECK(message.data.record.field_0.title.len == 7 && message.data.record.field_0.tags.len == 1, "mutable data enum owns the new record");
    demo_mutable_message_free(&message);
    return true;
}
