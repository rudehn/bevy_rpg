#!/usr/bin/env python3
"""
Encounter Coverage Report

Parses encounter_data.json and reports:
- Which machines can place on which floors
- Which machines have gaps (floors where a required tag has no eligible horde)
- Tag coverage per floor (what's available)
- Horde utilization (which hordes are never used by any machine)

Usage:
  python3 tools/encounter_coverage.py
  python3 tools/encounter_coverage.py --verbose
  python3 tools/encounter_coverage.py --floor 8
"""

import json
import sys
from pathlib import Path
from collections import defaultdict

TOTAL_FLOORS = 20


def load_data():
    data_path = Path(__file__).parent / "encounter_data.json"
    with open(data_path) as f:
        return json.load(f)


def get_eligible_hordes_by_floor(data):
    """For each floor, return dict of tag -> list of horde names."""
    floor_tags = {}
    for floor in range(1, TOTAL_FLOORS + 1):
        tags = defaultdict(list)
        for spawn in data["horde_spawns"]:
            if spawn["min_floor"] <= floor <= spawn["max_floor"]:
                horde_name = spawn["horde"]
                tag = data["hordes"][horde_name]["tag"]
                tags[tag].append(horde_name)
        floor_tags[floor] = dict(tags)
    return floor_tags


def check_machine_coverage(data, floor_tags):
    """For each machine, check which floors it can place on."""
    results = {}
    for name, machine in data["machines"].items():
        floors_ok = []
        floors_fail = {}
        for floor in range(machine["min_floor"], machine["max_floor"] + 1):
            if floor > TOTAL_FLOORS:
                continue
            missing = []
            for slot in machine["slots"]:
                tag = slot["tag"]
                if tag not in floor_tags.get(floor, {}):
                    missing.append(tag)
            if missing:
                floors_fail[floor] = missing
            else:
                floors_ok.append(floor)
        results[name] = {
            "min_floor": machine["min_floor"],
            "max_floor": min(machine["max_floor"], TOTAL_FLOORS),
            "slots": machine["slots"],
            "floors_ok": floors_ok,
            "floors_fail": floors_fail,
            "sub_machine_only": machine.get("sub_machine_only", False),
        }
    return results


def check_horde_utilization(data):
    """Find hordes whose tag is never referenced by any machine slot."""
    used_tags = set()
    for machine in data["machines"].values():
        for slot in machine["slots"]:
            used_tags.add(slot["tag"])

    unused = []
    for horde_name, horde in data["hordes"].items():
        if horde["tag"] not in used_tags:
            unused.append((horde_name, horde["tag"]))
    return unused, used_tags


def print_report(data, floor_tags, coverage, unused_hordes, used_tags, verbose=False, single_floor=None):
    print("=" * 70)
    print("ENCOUNTER COVERAGE REPORT")
    print("=" * 70)

    # --- Machine coverage ---
    print("\n## MACHINE COVERAGE\n")

    machines_with_gaps = []
    machines_ok = []

    for name, result in sorted(coverage.items()):
        if result["floors_fail"]:
            machines_with_gaps.append((name, result))
        else:
            machines_ok.append((name, result))

    if machines_with_gaps:
        print("### MACHINES WITH GAPS\n")
        for name, result in machines_with_gaps:
            prefix = "(sub-machine) " if result["sub_machine_only"] else ""
            print(f"  {prefix}{name} (floors {result['min_floor']}-{result['max_floor']})")
            tags_needed = [s["tag"] for s in result["slots"]]
            print(f"    Required tags: {', '.join(tags_needed)}")
            for floor, missing in sorted(result["floors_fail"].items()):
                print(f"    FLOOR {floor:2d}: MISSING [{', '.join(missing)}]")
            ok_range = result["floors_ok"]
            if ok_range:
                print(f"    OK on floors: {ok_range[0]}-{ok_range[-1]} ({len(ok_range)} floors)")
            print()
    else:
        print("  All machines have full coverage across their floor ranges!\n")

    print("### MACHINES WITH FULL COVERAGE\n")
    for name, result in machines_ok:
        prefix = "(sub-machine) " if result["sub_machine_only"] else ""
        tags_needed = [s["tag"] for s in result["slots"]] if result["slots"] else ["(none)"]
        print(f"  {prefix}{name} (floors {result['min_floor']}-{result['max_floor']}) "
              f"tags: [{', '.join(tags_needed)}]")
    print()

    # --- Tag coverage per floor ---
    print("## TAG AVAILABILITY PER FLOOR\n")

    all_tags = sorted(used_tags)
    header = f"  {'Floor':>5} | " + " | ".join(f"{t:>8}" for t in all_tags)
    print(header)
    print("  " + "-" * (len(header) - 2))

    for floor in range(1, TOTAL_FLOORS + 1):
        if single_floor and floor != single_floor:
            continue
        cells = []
        for tag in all_tags:
            hordes = floor_tags.get(floor, {}).get(tag, [])
            if hordes:
                cells.append(f"{len(hordes):>8}")
            else:
                cells.append(f"{'---':>8}")
        print(f"  {floor:>5} | " + " | ".join(cells))
    print()

    if verbose:
        print("## TAG DETAIL PER FLOOR\n")
        for floor in range(1, TOTAL_FLOORS + 1):
            if single_floor and floor != single_floor:
                continue
            print(f"  Floor {floor}:")
            tags = floor_tags.get(floor, {})
            if not tags:
                print("    (no hordes available)")
            for tag in sorted(tags.keys()):
                hordes = tags[tag]
                print(f"    {tag}: {', '.join(hordes)}")
            print()

    # --- Machine availability per floor ---
    print("## MACHINES AVAILABLE PER FLOOR\n")
    for floor in range(1, TOTAL_FLOORS + 1):
        if single_floor and floor != single_floor:
            continue
        available = []
        for name, result in sorted(coverage.items()):
            if result["sub_machine_only"]:
                continue
            if result["min_floor"] <= floor <= result["max_floor"] and floor not in result["floors_fail"]:
                available.append(name)
        print(f"  Floor {floor:2d}: {len(available)} machines — {', '.join(available)}")
    print()

    # --- Horde utilization ---
    print("## HORDE UTILIZATION\n")
    if unused_hordes:
        print("  Hordes with tags NOT referenced by any machine:\n")
        for horde_name, tag in unused_hordes:
            print(f"    {horde_name} (tag: {tag})")
        print()
    else:
        print("  All horde tags are referenced by at least one machine.\n")

    # --- Summary ---
    total_machines = len([m for m in coverage.values() if not m["sub_machine_only"]])
    gap_machines = len([m for m in machines_with_gaps if not m[1]["sub_machine_only"]])
    print("=" * 70)
    print(f"SUMMARY: {total_machines} machines ({gap_machines} with gaps), "
          f"{len(data['hordes'])} hordes ({len(unused_hordes)} unused tags)")
    print("=" * 70)


def main():
    verbose = "--verbose" in sys.argv or "-v" in sys.argv
    single_floor = None
    for arg in sys.argv[1:]:
        if arg.startswith("--floor"):
            idx = sys.argv.index(arg)
            if idx + 1 < len(sys.argv):
                single_floor = int(sys.argv[idx + 1])

    data = load_data()
    floor_tags = get_eligible_hordes_by_floor(data)
    coverage = check_machine_coverage(data, floor_tags)
    unused_hordes, used_tags = check_horde_utilization(data)
    print_report(data, floor_tags, coverage, unused_hordes, used_tags, verbose, single_floor)


if __name__ == "__main__":
    main()
