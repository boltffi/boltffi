plugins {
    kotlin("jvm") version "2.0.21"
    application
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
    testImplementation(kotlin("test"))
}

kotlin {
    jvmToolchain(17)
}

sourceSets {
    named("main") {
        kotlin.srcDirs("src/main/kotlin", "generated")
    }
}

val repoRoot = projectDir.resolve("../../..").canonicalFile
val demoDir = repoRoot.resolve("examples/demo")
val workspaceManifest = repoRoot.resolve("Cargo.toml")
val demoSource = demoDir.resolve("src/lib.rs")
val generatedDir = projectDir.resolve("generated")
val generatedJniDir = generatedDir.resolve("jni")
val rustLibraryPath = demoDir.resolve("target/debug/libdemo.dylib")
val nativeBuildDir = layout.buildDirectory.dir("native")
val javaHome = providers
    .environmentVariable("JAVA_HOME")
    .orElse(
        providers.exec {
            commandLine("/usr/libexec/java_home")
        }.standardOutput.asText.map(String::trim)
    )
    .get()
    .ifEmpty {
    error("JAVA_HOME is not set and /usr/libexec/java_home returned an empty result")
}

val generateKotlinBindings = tasks.register<Exec>("generateKotlinBindings") {
    workingDir = demoDir
    commandLine(
        "cargo",
        "run",
        "-q",
        "--manifest-path",
        workspaceManifest.absolutePath,
        "-p",
        "boltffi_cli",
        "--",
        // Transparent variants render for kotlin and python only, so the demo
        // keeps them behind a feature the other targets never enable.
        "--cargo-arg=--features",
        "--cargo-arg=transparent-demo",
        "generate",
        "kotlin",
        "--experimental",
    )
}

val buildDemoLibrary = tasks.register<Exec>("buildDemoLibrary") {
    workingDir = demoDir
    commandLine("cargo", "build", "-q", "--features", "transparent-demo")
    environment("BOLTFFI_BINDING_EXPANSION", "1")
    environment("BOLTFFI_BINDING_EXPANSION_ROOT", demoDir.absolutePath)
    environment("BOLTFFI_BINDING_EXPANSION_SOURCE", demoSource.absolutePath)
    environment("BOLTFFI_BINDING_EXPANSION_SURFACE", "native")
}

val buildJvmJniBridge = tasks.register("buildJvmJniBridge") {
    dependsOn(generateKotlinBindings, buildDemoLibrary)

    doLast {
        val nativeDirectory = nativeBuildDir.get().asFile.apply { mkdirs() }
        copy {
            from(rustLibraryPath)
            into(nativeDirectory)
        }
        providers.exec {
            commandLine(
                "clang",
                "-dynamiclib",
                "-I$javaHome/include",
                "-I$javaHome/include/darwin",
                "-I${generatedJniDir.absolutePath}",
                generatedJniDir.resolve("jni_glue.c").absolutePath,
                "-L${nativeDirectory.absolutePath}",
                "-ldemo",
                "-Wl,-rpath,@loader_path",
                "-o",
                nativeDirectory.resolve("libdemo_jni.dylib").absolutePath,
            )
        }.result.get().assertNormalExitValue()
    }
}

tasks.named("compileKotlin") {
    dependsOn(generateKotlinBindings)
}

tasks.named<JavaExec>("run") {
    dependsOn(buildJvmJniBridge)
    jvmArgs("-Djava.library.path=${nativeBuildDir.get().asFile.absolutePath}")
}

tasks.named<Test>("test") {
    dependsOn(buildJvmJniBridge)
    useJUnitPlatform()
    jvmArgs("-Djava.library.path=${nativeBuildDir.get().asFile.absolutePath}")
}

tasks.named("clean") {
    doLast {
        delete(generatedDir)
    }
}

application {
    mainClass = "com.boltffi.demo.SmokeKt"
}
