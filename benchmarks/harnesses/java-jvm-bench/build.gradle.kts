import java.io.File

plugins {
    java
    id("me.champeau.jmh") version "0.7.3"
}

group = "com.example"
version = "1.0-SNAPSHOT"

val uniffiDir = "${projectDir}/../../adapters/uniffi/target/release"
val boltffiJvmDir = "${projectDir}/../../generated/boltffi/dist/java"
val boltffiJavaSourceDir = file(boltffiJvmDir)
val nativePath = listOf(uniffiDir, boltffiJavaSourceDir.absolutePath)
    .joinToString(File.pathSeparator)

repositories {
    mavenCentral()
}

val buildUniffiJava by tasks.registering(Exec::class) {
    workingDir = projectDir
    commandLine("../../adapters/uniffi/build-java.sh")
}

val buildBoltffiJava by tasks.registering(Exec::class) {
    workingDir = projectDir
    commandLine("../../generated/boltffi/build-java.sh")
    outputs.upToDateWhen { false }
}

tasks.named("compileJava") {
    dependsOn(buildUniffiJava, buildBoltffiJava)
}

tasks.matching { it.name.startsWith("jmh") }.configureEach {
    dependsOn(buildUniffiJava, buildBoltffiJava)
}

val benchmarkJavaLauncher = javaToolchains.launcherFor {
    languageVersion = JavaLanguageVersion.of(25)
}
tasks.register("writeBenchmarkJavaLauncher") {
    val destination = layout.buildDirectory.file("java-launcher.txt")
    outputs.file(destination)
    doLast {
        destination.get().asFile.writeText(
            benchmarkJavaLauncher.get().executablePath.asFile.absolutePath + "\n",
        )
    }
}

tasks.named("jmh") {
    doFirst {
        file("${layout.buildDirectory.get()}/tmp/jmh/jmh.lock").delete()
    }
}

tasks.withType<JavaExec> {
    jvmArgs(
        "-Djava.library.path=$nativePath",
        "--enable-native-access=ALL-UNNAMED",
    )
}

jmh {
    jmhVersion = "1.37"
    fork = 1
    warmupIterations = 3
    iterations = 3
    warmup = "1s"
    timeOnIteration = "1s"
    resultFormat = "JSON"
    val include = providers.gradleProperty("jmhInclude").orNull
    if (include != null) {
        includes = listOf(include)
    }
    jvmArgsAppend = listOf(
        "-Djava.library.path=$nativePath",
        "--enable-native-access=ALL-UNNAMED",
    )
}

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
    sourceSets {
        named("main") {
            java.srcDir("${projectDir}/../../adapters/uniffi/dist/java")
            java.srcDir(boltffiJavaSourceDir)
        }
    }
}
