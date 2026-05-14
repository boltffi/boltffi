from __future__ import annotations

import re


METHOD_CASE_IDS: dict[str, str] = {
    "Noop": "noop",
    "EchoI32": "echo_i32",
    "Add": "add",
    "EchoStringSmall": "echo_string_small",
    "EchoString64K": "echo_string_64k",
    "GenerateString1K": "generate_string_1k",
    "GenerateString64K": "generate_string_64k",
    "MakePoint": "make_point",
    "EchoAddress": "echo_address",
    "EchoPerson": "echo_person",
    "EchoLine": "echo_line",
    "EchoDirection_North": "echo_direction_north",
    "EchoDirection_West": "echo_direction_west",
    "EchoTaskStatus_UnitVariant": "echo_task_status_unit_variant",
    "EchoTaskStatus_SmallPayload": "echo_task_status_small_payload",
    "EchoTaskStatus_CompletedPayload": "echo_task_status_completed_payload",
}


def method_name_to_case_id(method_name: str) -> str:
    known_case_id = METHOD_CASE_IDS.get(method_name)
    if known_case_id is not None:
        return known_case_id

    normalized_name = re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", method_name).lower()
    normalized_name = re.sub(r"([a-z])((?:100|64|10|1)_k)\b", r"\1_\2", normalized_name)
    for scale in ("100", "64", "10", "1"):
        normalized_name = normalized_name.replace(f"{scale}_k", f"{scale}k")
    return normalized_name
