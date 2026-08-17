import 'package:demo_dart/testing.dart';
import 'package:test/test.dart';
import 'package:demo/demo.dart';

void main() {
  tearDownAll(shutdownBoltffi);
  test('records with defaults', () {
    expect(
      requestTimeoutSeconds(RequestConfig()),
      1.5,
      reason:
          "case:records.default_values.custom_type.should_apply_default",
    );

    final fromOwnedName = ServiceConfig.fromOwnedName('owned');
    expect(
      fromOwnedName.name,
      'owned',
      reason:
          "case:records.default_values.service_config.from_owned_name.should_return_config",
    );

    final fromBorrowedName = ServiceConfig.fromBorrowedName('borrowed');
    expect(
      fromBorrowedName.name,
      'borrowed',
      reason:
          "case:records.default_values.service_config.from_borrowed_name.should_return_config",
    );

    final fromStringRefName = ServiceConfig.fromStringRefName('stringref');
    expect(
      fromStringRefName.name,
      'stringref',
      reason:
          "case:records.default_values.service_config.from_string_ref_name.should_return_config",
    );

    final implicitDefaults = ServiceConfig(
      name: 'worker',
      backupEndpoint: 'https://default',
    );
    expect(implicitDefaults.name, 'worker', reason: "case:");
    expect(implicitDefaults.retries, 3);
    expect(implicitDefaults.region, 'standard');
    expect(implicitDefaults.endpoint, isNull);
    expect(implicitDefaults.backupEndpoint, 'https://default');

    final customRetries = ServiceConfig(name: 'worker', retries: 7);
    expect(customRetries.name, 'worker');
    expect(customRetries.retries, 7);
    expect(customRetries.region, 'standard');
    expect(customRetries.endpoint, isNull);
    expect(customRetries.backupEndpoint, 'https://default');

    final explicitRegion = ServiceConfig(
      name: 'worker',
      retries: 9,
      region: 'eu-west',
    );
    expect(explicitRegion.endpoint, isNull);
    expect(explicitRegion.backupEndpoint, 'https://default');

    final explicitEndpoint = ServiceConfig(
      name: 'worker',
      retries: 9,
      region: 'eu-west',
      endpoint: 'https://edge',
    );
    expect(explicitEndpoint.backupEndpoint, 'https://default');

    final explicitBackupEndpoint = ServiceConfig(
      name: 'worker',
      retries: 9,
      region: 'eu-west',
      endpoint: 'https://edge',
      backupEndpoint: 'https://backup',
    );
    expect(
      echoServiceConfig(explicitBackupEndpoint),
      explicitBackupEndpoint,
      reason:
          "case:records.default_values.service_config.should_roundtrip_value",
    );

    expect(
      implicitDefaults.describe(),
      'worker:3:standard:none:https://default',
    );
    expect(customRetries.describe(), 'worker:7:standard:none:https://default');
    expect(explicitRegion.describe(), 'worker:9:eu-west:none:https://default');
    expect(
      explicitEndpoint.describe(),
      'worker:9:eu-west:https://edge:https://default',
    );
    expect(
      explicitBackupEndpoint.describe(),
      'worker:9:eu-west:https://edge:https://backup',
    );

    expect(
      explicitBackupEndpoint.describeWithPrefix('cfg'),
      'cfg:worker:9:eu-west:https://edge:https://backup',
      reason:
          "case:records.default_values.service_config.should_describe_with_prefix",
    );
    expect(fromOwnedName.describe(), 'owned:3:standard:none:https://default');
    expect(
      fromBorrowedName.describe(),
      'borrowed:3:standard:none:https://default',
    );
    expect(
      fromStringRefName.describe(),
      'stringref:3:standard:none:https://default',
      reason:
          "case:records.default_values.service_config.should_describe_values",
    );

    final tryWithRetries = ServiceConfig.tryWithRetries(3);
    expect(
      tryWithRetries.retries,
      3,
      reason:
          "case:records.default_values.service_config.try_with_retries.should_return_config",
    );
    expect(
      () => ServiceConfig.tryWithRetries(-1),
      throwsBoltException("service config retries must be non-negative"),
      reason:
          "case:records.default_values.service_config.try_with_retries.should_reject_negative_retries",
    );

    final maybeWithRetries = ServiceConfig.maybeWithRetries(3);
    expect(
      maybeWithRetries!.retries,
      3,
      reason:
          "case:records.default_values.service_config.maybe_with_retries.should_return_some",
    );
    expect(
      ServiceConfig.maybeWithRetries(-1),
      isNull,
      reason:
          "case:records.default_values.service_config.maybe_with_retries.should_return_none",
    );

    final fromNonEmptyName = ServiceConfig.fromNonEmptyName(
      'nonempty',
      'region',
    );
    expect(
      fromNonEmptyName!.name,
      'nonempty',
      reason:
          "case:records.default_values.service_config.from_non_empty_name.should_return_config_for_non_empty_values",
    );
    expect(
      ServiceConfig.fromNonEmptyName('', 'region'),
      isNull,
      reason:
          "case:records.default_values.service_config.from_non_empty_name.should_return_none_for_empty_values",
    );
  });
}
