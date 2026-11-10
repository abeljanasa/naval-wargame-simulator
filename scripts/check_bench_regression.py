#!/usr/bin/env python3
"""Fail if any criterion benchmark got more than THRESHOLD slower than the saved baseline.

Usage (CI runs both benchmark sets on the same runner, so runner speed cancels out):
    cargo bench -p sim-scenario --bench engine -- --save-baseline base   # on the base commit
    cargo bench -p sim-scenario --bench engine -- --baseline base        # on the change
    python3 scripts/check_bench_regression.py [target/criterion] [--threshold 0.15]

Criterion writes <bench>/change/estimates.json when comparing against a baseline; the mean's
point estimate is the relative change in time (0.20 = 20 % slower). A benchmark is a regression
only if both the point estimate and the lower confidence bound exceed the threshold, so noise
alone does not fail the build.
"""

import json
import pathlib
import sys


def main(argv):
    root = pathlib.Path("target/criterion")
    threshold = 0.15
    args = list(argv)
    while args:
        a = args.pop(0)
        if a == "--threshold":
            threshold = float(args.pop(0))
        else:
            root = pathlib.Path(a)

    changes = sorted(root.glob("**/change/estimates.json"))
    if not changes:
        print(f"no criterion comparisons under {root}; nothing to check")
        return 0
    failed = 0
    for path in changes:
        bench = path.parent.parent.relative_to(root).as_posix()
        mean = json.loads(path.read_text())["mean"]
        point = mean["point_estimate"]
        lower = mean["confidence_interval"]["lower_bound"]
        regressed = point > threshold and lower > threshold
        failed += regressed
        status = "REGRESSION" if regressed else "ok"
        print(f"{status:<10} {bench:<45} {point:+7.1%} (95% CI lower {lower:+7.1%})")
    if failed:
        print(f"{failed} benchmark(s) more than {threshold:.0%} slower than the base")
        return 1
    print(f"no benchmark more than {threshold:.0%} slower than the base")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
