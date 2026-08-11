import 'dart:io';

import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';
import 'package:logging/logging.dart';
import 'package:path/path.dart' as path;

String targetTripleFromCodeConfig(CodeConfig codeConfig) {
  return switch ((codeConfig.targetOS, codeConfig.targetArchitecture)) {
    (OS.android, Architecture.arm64) => 'aarch64-linux-android',
    (OS.android, Architecture.arm) => 'armv7-linux-androideabi',
    (OS.android, Architecture.x64) => 'x86_64-linux-android',

    (OS.iOS, Architecture.arm64)
        when codeConfig.iOS.targetSdk == IOSSdk.iPhoneSimulator =>
      'aarch64-apple-ios-sim',
    (OS.iOS, Architecture.arm64)
        when codeConfig.iOS.targetSdk == IOSSdk.iPhoneOS =>
      'aarch64-apple-ios',
    (OS.iOS, Architecture.arm64) => throw UnsupportedError(
      'Unknown IOSSdk: ${codeConfig.iOS.targetSdk}',
    ),
    (OS.iOS, Architecture.x64) => 'x86_64-apple-ios',

    (OS.linux, Architecture.arm64) => 'aarch64-unknown-linux-gnu',
    (OS.linux, Architecture.x64) => 'x86_64-unknown-linux-gnu',

    (OS.macOS, Architecture.arm64) => 'aarch64-apple-darwin',
    (OS.macOS, Architecture.x64) => 'x86_64-apple-darwin',

    (OS.windows, Architecture.arm64) => 'aarch64-pc-windows-msvc',
    (OS.windows, Architecture.x64) => 'x86_64-pc-windows-msvc',

    (_, _) => throw UnsupportedError(
      'Unsupported target: $codeConfig.targetOS on $codeConfig.targetArchitecture',
    ),
  };
}

LinkMode linkModeFromCodeConfig(CodeConfig codeConfig) {
  return switch (codeConfig.linkModePreference) {
    LinkModePreference.dynamic ||
    LinkModePreference.preferDynamic => DynamicLoadingBundled(),
    LinkModePreference.static ||
    LinkModePreference.preferStatic => StaticLinking(),
    _ => throw UnsupportedError(
      'Unsupported LinkModePreference: $codeConfig.linkModePreference',
    ),
  };
}

final class BoltFFIBuilder implements Builder {
  @override
  Future<void> run({
    required BuildInput input,
    required BuildOutputBuilder output,
    Logger? logger,
  }) async {
    logger ??= Logger("BoltFFI::build");

    if (!input.config.buildCodeAssets) {
      logger.info(
        'buildCodeAssets is false; '
        'skipping build of Rust code assets',
      );
      return;
    }

    const artifactName = "{{ artifact_name }}";
    const assetName = "{{ artifact_name }}.dart";

    final codeConfig = input.config.code;
    final targetTriple = targetTripleFromCodeConfig(codeConfig);
    final linkMode = linkModeFromCodeConfig(codeConfig);

    final nativeDir = input.packageRoot.resolve("native");
    final libFile = Uri.file(
      path.join(
        nativeDir.toFilePath(),
        targetTriple,
        codeConfig.targetOS
            .libraryFileName(artifactName, linkMode)
            .replaceAll('-', '_'),
      ),
    );

    if (!await File(libFile.toFilePath()).exists()) {
      throw Exception('''
Library for target `$targetTriple` not found.
      ''');
    }

    final outputNativeDir = input.outputDirectory.resolve("native");
    await Directory(outputNativeDir.toFilePath()).create();

    final outputLibFile = Uri.file(
      path.join(
        outputNativeDir.toFilePath(),
        codeConfig.targetOS
            .libraryFileName(artifactName, linkMode)
            .replaceAll('-', '_'),
      ),
    );
    await File(libFile.toFilePath()).copy(outputLibFile.toFilePath());

    output.dependencies.addAll(
      await Directory(nativeDir.toFilePath())
          .list(recursive: true)
          .where((e) => e is File)
          .map((f) => f.uri)
          .toList(),
    );

    output.assets.code.add(
      CodeAsset(
        package: input.packageName,
        name: assetName,
        linkMode: linkMode,
        file: outputLibFile,
      ),
      routing: const ToAppBundle(),
    );
  }
}

void main(List<String> args) async {
  await build(args, (input, output) async {
    await BoltFFIBuilder().run(input: input, output: output);
  });
}
