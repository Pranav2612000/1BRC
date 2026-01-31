#!/usr/bin/env python3
"""
Script to compare two result files from the 1BRC challenge.
City names can be in any order and floating point values are compared with ±0.01 tolerance.
"""

import sys
import re


def parse_results_file(filepath):
    """Parse a results file and return a dictionary of city -> (min, mean, max)."""
    cities = {}

    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read().strip()

    # Check if it's the old format (enclosed in braces) or new format (line-by-line)
    if content.startswith('{') and content.endswith('}'):
        # Old format: {city1=min/mean/max, city2=min/mean/max, ...}
        content = content[1:-1]
        pattern = r'([^=]+)=([-\d.]+)/([-\d.]+)/([-\d.]+)'
        matches = re.finditer(pattern, content)
        for match in matches:
            city = match.group(1).strip()
            min_temp = float(match.group(2))
            mean_temp = float(match.group(3))
            max_temp = float(match.group(4))
            cities[city] = (min_temp, mean_temp, max_temp)
    else:
        # New format: line-by-line (CityName=min/mean/max per line)
        for line in content.split('\n'):
            line = line.strip()
            if not line:
                continue

            # Pattern: CityName=min/mean/max
            match = re.match(r'^([^=]+)=([-\d.]+)/([-\d.]+)/([-\d.]+)$', line)
            if match:
                city = match.group(1).strip()
                min_temp = float(match.group(2))
                mean_temp = float(match.group(3))
                max_temp = float(match.group(4))
                cities[city] = (min_temp, mean_temp, max_temp)

    return cities


def compare_values(val1, val2, tolerance=0.01):
    """Compare two float values with given tolerance."""
    return abs(val1 - val2) <= tolerance


def compare_results(file1, file2, tolerance=0.01):
    """Compare two result files and report differences."""
    print(f"Comparing {file1} and {file2}")
    print(f"Tolerance: ±{tolerance}")
    print("-" * 80)

    cities1 = parse_results_file(file1)
    cities2 = parse_results_file(file2)

    print(f"File 1 has {len(cities1)} cities")
    print(f"File 2 has {len(cities2)} cities")
    print("-" * 80)

    # Check for cities only in file1
    only_in_file1 = set(cities1.keys()) - set(cities2.keys())
    if only_in_file1:
        print(f"\n❌ Cities only in file 1: {len(only_in_file1)}")
        for city in sorted(only_in_file1)[:10]:  # Show first 10
            print(f"  - {city}")
        if len(only_in_file1) > 10:
            print(f"  ... and {len(only_in_file1) - 10} more")

    # Check for cities only in file2
    only_in_file2 = set(cities2.keys()) - set(cities1.keys())
    if only_in_file2:
        print(f"\n❌ Cities only in file 2: {len(only_in_file2)}")
        for city in sorted(only_in_file2)[:10]:  # Show first 10
            print(f"  - {city}")
        if len(only_in_file2) > 10:
            print(f"  ... and {len(only_in_file2) - 10} more")

    # Compare values for common cities
    common_cities = set(cities1.keys()) & set(cities2.keys())
    differences = []

    for city in common_cities:
        min1, mean1, max1 = cities1[city]
        min2, mean2, max2 = cities2[city]

        min_ok = compare_values(min1, min2, tolerance)
        mean_ok = compare_values(mean1, mean2, tolerance)
        max_ok = compare_values(max1, max2, tolerance)

        if not (min_ok and mean_ok and max_ok):
            diff = {
                'city': city,
                'file1': (min1, mean1, max1),
                'file2': (min2, mean2, max2),
                'min_diff': abs(min1 - min2),
                'mean_diff': abs(mean1 - mean2),
                'max_diff': abs(max1 - max2)
            }
            differences.append(diff)

    if differences:
        print(f"\n❌ Value differences found: {len(differences)} cities")
        print("-" * 80)
        for diff in differences[:20]:  # Show first 20
            city = diff['city']
            min1, mean1, max1 = diff['file1']
            min2, mean2, max2 = diff['file2']
            print(f"\n{city}:")
            print(f"  File 1: {min1}/{mean1}/{max1}")
            print(f"  File 2: {min2}/{mean2}/{max2}")
            print(f"  Diff:   {diff['min_diff']:.4f}/{diff['mean_diff']:.4f}/{diff['max_diff']:.4f}")

        if len(differences) > 20:
            print(f"\n  ... and {len(differences) - 20} more cities with differences")

    # Summary
    print("\n" + "=" * 80)
    if only_in_file1 or only_in_file2 or differences:
        print("❌ COMPARISON FAILED")
        print(f"  - Cities only in file 1: {len(only_in_file1)}")
        print(f"  - Cities only in file 2: {len(only_in_file2)}")
        print(f"  - Cities with value differences: {len(differences)}")
        return False
    else:
        print("✅ COMPARISON PASSED")
        print(f"  - All {len(common_cities)} cities match within tolerance ±{tolerance}")
        return True


if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: python compare_results.py <file1> <file2> [tolerance]")
        print("  tolerance: optional, default is 0.01")
        sys.exit(1)

    file1 = sys.argv[1]
    file2 = sys.argv[2]
    tolerance = float(sys.argv[3]) if len(sys.argv) > 3 else 0.01

    success = compare_results(file1, file2, tolerance)
    sys.exit(0 if success else 1)
