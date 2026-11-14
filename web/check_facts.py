# SPDX-FileCopyrightText: 2026 SynrgStudio
# SPDX-License-Identifier: Apache-2.0

"""Checks the numbers in facts.toml against the data they come from.

    python web/check_facts.py

A fact with `derive` says how its value is computed: a Python expression
over the repository's files, with these helpers:

    data(name)        the JSON of web/data/<name>
    models(name)      its "models" list
    files(pattern)    how many files match a glob, from the repository's root
    toml(path)        a TOML file of the repository
    median(xs), percentile(xs, p), mb(n), gb(n)

The value written must be the computed one, rounded to as many decimals as it
is written with. A fact without `derive` was measured once, in an issue, a PR
or a run that left no data file; the script lists those, so it is clear which
numbers anyone can check and which they cannot.

It needs nothing but Python 3.11 or later. It exits with 1 when a value is
not what its data says.
"""

import glob
import json
import math
import statistics
import sys
import tomllib
from functools import cache
from pathlib import Path

WEB = Path(__file__).resolve().parent
ROOT = WEB.parent


@cache
def data(name):
    return json.loads((WEB / "data" / name).read_text(encoding="utf-8"))


def models(name):
    return data(name)["models"]


def files(pattern):
    return len(glob.glob(pattern, root_dir=ROOT))


def toml(path):
    return tomllib.loads((ROOT / path).read_text(encoding="utf-8"))


def percentile(xs, p):
    """The p-th percentile, by the nearest rank: the smallest value with at
    least p% of the values at or below it."""
    xs = sorted(xs)
    return xs[max(0, math.ceil(p / 100 * len(xs)) - 1)]


HELPERS = {
    "data": data,
    "models": models,
    "files": files,
    "toml": toml,
    "median": statistics.median,
    "percentile": percentile,
    "mb": lambda n: n / 1e6,
    "gb": lambda n: n / 1e9,
    "sum": sum,
    "len": len,
    "min": min,
    "max": max,
    "round": round,
}


def decimals(value):
    if isinstance(value, int):
        return 0
    text = repr(value)
    return len(text.split(".")[1]) if "." in text else 0


def main():
    facts = tomllib.loads((WEB / "facts.toml").read_text(encoding="utf-8"))["facts"]
    checked, wrong, unchecked = 0, [], []
    for key, fact in facts.items():
        expression = fact.get("derive")
        if expression is None:
            unchecked.append(key)
            continue
        computed = eval(expression, {"__builtins__": {}, **HELPERS})
        places = decimals(fact["value"])
        rounded = round(computed, places) if places else round(computed)
        checked += 1
        if rounded != fact["value"]:
            wrong.append(f"{key}: written {fact['value']}, the data gives {computed!r}")
    print(f"{checked} facts checked against their data, {len(wrong)} wrong.")
    for line in wrong:
        print(f"  WRONG {line}")
    print(f"{len(unchecked)} facts have no data file behind them (measured once, see their source):")
    for key in unchecked:
        print(f"  {key}: {facts[key].get('source', '')}")
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
