from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass(frozen=True)
class BenchmarkCaseSpec:
    canonical_name: str
    group: str
    title: str
    category: str
    sophistication: str
    direction: str
    parameters: dict[str, Any] = field(default_factory=dict)
    description: str | None = None
    tags: tuple[str, ...] = ()
    aliases: tuple[str, ...] = ()


def _case(
    canonical_name: str,
    *,
    group: str,
    title: str,
    category: str,
    sophistication: str = "basic",
    direction: str = "mixed",
    parameters: dict[str, Any] | None = None,
    description: str | None = None,
    tags: tuple[str, ...] = (),
    aliases: tuple[str, ...] = (),
) -> BenchmarkCaseSpec:
    return BenchmarkCaseSpec(
        canonical_name=canonical_name,
        group=group,
        title=title,
        category=category,
        sophistication=sophistication,
        direction=direction,
        parameters=parameters or {},
        description=description,
        tags=tags,
        aliases=aliases,
    )


def _scale_suffix(count: int) -> str:
    if count % 1000 == 0:
        return f"{count // 1000}k"
    return str(count)


def _build_catalog() -> tuple[BenchmarkCaseSpec, ...]:
    cases: list[BenchmarkCaseSpec] = [
        _case("noop", group="primitives.noop", title="Noop", category="primitives"),
        _case("echo_bool", group="primitives.echo_bool", title="Echo Bool", category="primitives"),
        _case("negate_bool", group="primitives.negate_bool", title="Negate Bool", category="primitives"),
        _case("echo_i8", group="primitives.echo_i8", title="Echo I8", category="primitives"),
        _case("echo_u8", group="primitives.echo_u8", title="Echo U8", category="primitives"),
        _case("echo_i16", group="primitives.echo_i16", title="Echo I16", category="primitives"),
        _case("echo_u16", group="primitives.echo_u16", title="Echo U16", category="primitives"),
        _case("echo_i32", group="primitives.echo_i32", title="Echo I32", category="primitives"),
        _case("add_i32", group="primitives.add_i32", title="Add I32", category="primitives"),
        _case("echo_u32", group="primitives.echo_u32", title="Echo U32", category="primitives"),
        _case("echo_i64", group="primitives.echo_i64", title="Echo I64", category="primitives"),
        _case("echo_u64", group="primitives.echo_u64", title="Echo U64", category="primitives"),
        _case("echo_f32", group="primitives.echo_f32", title="Echo F32", category="primitives"),
        _case("add_f32", group="primitives.add_f32", title="Add F32", category="primitives"),
        _case("echo_f64", group="primitives.echo_f64", title="Echo F64", category="primitives"),
        _case("add", group="primitives.add", title="Add", category="primitives"),
        _case("add_f64", group="primitives.add_f64", title="Add F64", category="primitives"),
        _case(
            "echo_usize",
            group="primitives.echo_usize",
            title="Echo Usize",
            category="primitives",
        ),
        _case(
            "echo_isize",
            group="primitives.echo_isize",
            title="Echo Isize",
            category="primitives",
        ),
        _case("multiply", group="primitives.multiply", title="Multiply", category="primitives"),
        _case(
            "inc_u64",
            group="primitives.inc_u64.in_place",
            title="Increment U64 In Place",
            category="primitives",
            parameters={"mutation_mode": "in_place"},
        ),
        _case(
            "inc_u64_value",
            group="primitives.inc_u64.by_value",
            title="Increment U64 By Value",
            category="primitives",
            parameters={"mutation_mode": "by_value"},
        ),
        _case(
            "echo_string_small",
            group="strings.echo",
            title="Echo String Small",
            category="strings",
            parameters={"string_length": 5},
        ),
        _case(
            "echo_string_200",
            group="strings.echo",
            title="Echo String 200",
            category="strings",
            parameters={"string_length": 200},
        ),
        _case(
            "echo_string_1k",
            group="strings.echo",
            title="Echo String 1K",
            category="strings",
            parameters={"string_length": 1000},
        ),
        _case(
            "echo_string_64k",
            group="strings.echo",
            title="Echo String 64K",
            category="strings",
            parameters={"string_length": 65536},
        ),
        _case(
            "generate_string_1k",
            group="strings.generate",
            title="Generate String 1K",
            category="strings",
            direction="rust_to_host",
            parameters={"string_length": 1000},
        ),
        _case(
            "generate_string_64k",
            group="strings.generate",
            title="Generate String 64K",
            category="strings",
            direction="rust_to_host",
            parameters={"string_length": 65536},
        ),
        _case(
            "echo_bytes_64k",
            group="bytes.echo",
            title="Echo Bytes 64K",
            category="bytes",
            direction="roundtrip",
            parameters={"size_bytes": 65536},
        ),
        _case(
            "generate_bytes_64k",
            group="bytes.generate",
            title="Generate Bytes 64K",
            category="bytes",
            direction="rust_to_host",
            parameters={"size_bytes": 65536},
        ),
        _case(
            "simple_enum",
            group="enums.direction.basic_ops",
            title="Simple Enum",
            category="enums",
            description="Calls oppositeDirection and directionToDegrees with simple enum values.",
        ),
        _case(
            "data_enum_input",
            group="enums.task_status.input",
            title="Data Enum Input",
            category="enums",
            description="Passes data-carrying enum values into Rust and inspects the result.",
        ),
        _case(
            "find_even_100",
            group="options.find_even",
            title="Find Even 100",
            category="options",
            parameters={"count": 100},
            aliases=("find_even",),
        ),
        _case(
            "find_positive_f64",
            group="options.find_positive_f64",
            title="Find Positive F64",
            category="options",
            parameters={"value": 3.14},
        ),
        _case(
            "find_name",
            group="options.find_name",
            title="Find Name",
            category="options",
            parameters={"id": 1},
        ),
        _case(
            "find_names_100",
            group="options.find_names",
            title="Find Names 100",
            category="options",
            parameters={"count": 100},
        ),
        _case(
            "find_numbers_100",
            group="options.find_numbers",
            title="Find Numbers 100",
            category="options",
            parameters={"count": 100},
        ),
        _case(
            "async_add",
            group="async_fns.add",
            title="Async Add",
            category="async_fns",
            sophistication="async",
        ),
        _case(
            "counter_increment_mutex",
            group="classes.counter.increment",
            title="Counter Increment Mutex",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"iterations": 1000, "synchronization": "mutex"},
            aliases=("counter_increment (mutex)", "counter_increment_1k (mutex)"),
        ),
        _case(
            "counter_increment_single_threaded",
            group="classes.counter.increment",
            title="Counter Increment Single Threaded",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"iterations": 1000, "synchronization": "single_threaded"},
            aliases=("counter_increment_1k", "counter_increment (single_threaded, BoltFFI-only)"),
        ),
        _case(
            "datastore_add_record_1k",
            group="classes.datastore.add",
            title="Datastore Add Record 1K",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"count": 1000, "input_shape": "record"},
            aliases=("datastore_add", "datastore_add_1k"),
        ),
        _case(
            "datastore_add_scalars_1k",
            group="classes.datastore.add",
            title="Datastore Add Scalars 1K",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"count": 1000, "input_shape": "scalar_fields"},
        ),
        _case(
            "accumulator_mutex",
            group="classes.accumulator.add",
            title="Accumulator Mutex",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"iterations": 1000, "synchronization": "mutex"},
            aliases=("accumulator_1k (mutex)",),
        ),
        _case(
            "accumulator_single_threaded",
            group="classes.accumulator.add",
            title="Accumulator Single Threaded",
            category="classes",
            sophistication="complex",
            direction="host_to_rust",
            parameters={"iterations": 1000, "synchronization": "single_threaded"},
            aliases=("accumulator_1k",),
        ),
        _case(
            "callback_100",
            group="callbacks.data_provider.compute_sum",
            title="Callback 100",
            category="callbacks",
            sophistication="callback",
            direction="callback",
            parameters={"count": 100},
        ),
        _case(
            "callback_1k",
            group="callbacks.data_provider.compute_sum",
            title="Callback 1K",
            category="callbacks",
            sophistication="callback",
            direction="callback",
            parameters={"count": 1000},
        ),
        _case(
            "roundtrip_locations_100",
            group="records.locations.roundtrip",
            title="Roundtrip Locations 100",
            category="records",
            sophistication="structured",
            direction="roundtrip",
            parameters={"count": 100},
        ),
        _case(
            "roundtrip_i32_vec_1k",
            group="collections.i32_vec.roundtrip",
            title="Roundtrip I32 Vec 1K",
            category="collections",
            sophistication="structured",
            direction="roundtrip",
            parameters={"count": 1000},
        ),
        _case(
            "echo_vec_i32_10k",
            group="collections.i32_vec.echo",
            title="Echo I32 Vec 10K",
            category="collections",
            sophistication="structured",
            direction="roundtrip",
            parameters={"count": 10000},
        ),
        _case(
            "echo_direction",
            group="enums.direction.echo",
            title="Echo Direction",
            category="enums",
        ),
        _case(
            "echo_direction_north",
            group="enums.direction.echo",
            title="Echo Direction North",
            category="enums",
            parameters={"value": "north"},
        ),
        _case(
            "echo_direction_west",
            group="enums.direction.echo",
            title="Echo Direction West",
            category="enums",
            parameters={"value": "west"},
        ),
        _case(
            "echo_task_status_unit_variant",
            group="enums.task_status.echo",
            title="Echo TaskStatus Unit Variant",
            category="enums",
            direction="roundtrip",
            parameters={"variant": "pending"},
        ),
        _case(
            "echo_task_status_small_payload",
            group="enums.task_status.echo",
            title="Echo TaskStatus Small Payload",
            category="enums",
            direction="roundtrip",
            parameters={"variant": "in_progress"},
        ),
        _case(
            "echo_task_status_completed_payload",
            group="enums.task_status.echo",
            title="Echo TaskStatus Completed Payload",
            category="enums",
            direction="roundtrip",
            parameters={"variant": "completed"},
        ),
        _case(
            "find_direction",
            group="enums.direction.find",
            title="Find Direction",
            category="enums",
            parameters={"id": 0},
        ),
        _case(
            "find_locations_100",
            group="records.locations.find",
            title="Find Locations 100",
            category="records",
            sophistication="structured",
            direction="rust_to_host",
            parameters={"count": 100},
        ),
        _case(
            "make_point",
            group="records.point.make",
            title="Make Point",
            category="records",
            sophistication="structured",
            direction="rust_to_host",
        ),
        _case(
            "echo_address",
            group="records.address.echo",
            title="Echo Address",
            category="records",
            sophistication="structured",
            direction="roundtrip",
        ),
        _case(
            "echo_person",
            group="records.person.echo",
            title="Echo Person",
            category="records",
            sophistication="structured",
            direction="roundtrip",
        ),
        _case(
            "echo_line",
            group="records.line.echo",
            title="Echo Line",
            category="records",
            sophistication="structured",
            direction="roundtrip",
        ),
    ]

    for count in (100, 1000, 10000):
        suffix = _scale_suffix(count)
        cases.extend(
            [
                _case(
                    f"generate_locations_{suffix}",
                    group="records.locations.generate",
                    title=f"Generate Locations {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="rust_to_host",
                    parameters={"count": count},
                ),
                _case(
                    f"generate_trades_{suffix}",
                    group="records.trades.generate",
                    title=f"Generate Trades {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="rust_to_host",
                    parameters={"count": count},
                ),
                _case(
                    f"generate_particles_{suffix}",
                    group="records.particles.generate",
                    title=f"Generate Particles {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="rust_to_host",
                    parameters={"count": count},
                ),
                _case(
                    f"generate_sensor_readings_{suffix}",
                    group="records.sensor_readings.generate",
                    title=f"Generate Sensor Readings {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="rust_to_host",
                    parameters={"count": count},
                    aliases=(f"generate_sensors_{suffix}",),
                ),
            ]
        )

    for count in (1000, 10000):
        suffix = _scale_suffix(count)
        cases.extend(
            [
                _case(
                    f"sum_ratings_{suffix}",
                    group="records.locations.sum_ratings",
                    title=f"Sum Ratings {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"sum_trade_volumes_{suffix}",
                    group="records.trades.sum_trade_volumes",
                    title=f"Sum Trade Volumes {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"sum_particle_masses_{suffix}",
                    group="records.particles.sum_particle_masses",
                    title=f"Sum Particle Masses {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"avg_sensor_temp_{suffix}",
                    group="records.sensor_readings.average_temperature",
                    title=f"Average Sensor Temperature {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"process_locations_{suffix}",
                    group="records.locations.process",
                    title=f"Process Locations {suffix.upper()}",
                    category="records",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"generate_directions_{suffix}",
                    group="enums.directions.generate",
                    title=f"Generate Directions {suffix.upper()}",
                    category="enums",
                    sophistication="structured",
                    direction="rust_to_host",
                    parameters={"count": count},
                ),
                _case(
                    f"count_north_{suffix}",
                    group="enums.directions.count_north",
                    title=f"Count North {suffix.upper()}",
                    category="enums",
                    sophistication="structured",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
            ]
        )

    for count in (1000, 10000, 100000):
        suffix = _scale_suffix(count)
        cases.append(
            _case(
                f"generate_i32_vec_{suffix}",
                group="collections.i32_vec.generate",
                title=f"Generate I32 Vec {suffix.upper()}",
                category="collections",
                sophistication="structured",
                direction="rust_to_host",
                parameters={"count": count},
            )
        )

    for count in (1000, 10000):
        suffix = _scale_suffix(count)
        cases.append(
            _case(
                f"sum_i32_vec_{suffix}",
                group="collections.i32_vec.sum",
                title=f"Sum I32 Vec {suffix.upper()}",
                category="collections",
                sophistication="structured",
                direction="host_to_rust",
                parameters={"count": count},
            )
        )

    cases.extend(
        [
            _case(
                "sum_i32_vec_100k",
                group="collections.i32_vec.sum",
                title="Sum I32 Vec 100K",
                category="collections",
                sophistication="structured",
                direction="host_to_rust",
                parameters={"count": 100000},
            ),
            _case(
                "generate_f64_vec_10k",
                group="collections.f64_vec.generate",
                title="Generate F64 Vec 10K",
                category="collections",
                sophistication="structured",
                direction="rust_to_host",
                parameters={"count": 10000},
            ),
            _case(
                "sum_f64_vec_10k",
                group="collections.f64_vec.sum",
                title="Sum F64 Vec 10K",
                category="collections",
                sophistication="structured",
                direction="host_to_rust",
                parameters={"count": 10000},
            ),
        ]
    )

    for count in (100, 1000):
        suffix = _scale_suffix(count)
        cases.extend(
            [
                _case(
                    f"generate_user_profiles_{suffix}",
                    group="records.user_profiles.generate",
                    title=f"Generate User Profiles {suffix.upper()}",
                    category="records",
                    sophistication="complex",
                    direction="rust_to_host",
                    parameters={"count": count},
                ),
                _case(
                    f"sum_user_scores_{suffix}",
                    group="records.user_profiles.sum_user_scores",
                    title=f"Sum User Scores {suffix.upper()}",
                    category="records",
                    sophistication="complex",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
                _case(
                    f"count_active_users_{suffix}",
                    group="records.user_profiles.count_active_users",
                    title=f"Count Active Users {suffix.upper()}",
                    category="records",
                    sophistication="complex",
                    direction="host_to_rust",
                    parameters={"count": count},
                ),
            ]
        )

    return tuple(cases)


BENCHMARK_CATALOG = _build_catalog()

_CASE_LOOKUP: dict[str, BenchmarkCaseSpec] = {}
for case in BENCHMARK_CATALOG:
    for name in (case.canonical_name, *case.aliases):
        _CASE_LOOKUP[name] = case


def lookup_case_spec(case_name: str) -> BenchmarkCaseSpec | None:
    return _CASE_LOOKUP.get(case_name)


def known_case_names() -> tuple[str, ...]:
    return tuple(sorted(_CASE_LOOKUP))
