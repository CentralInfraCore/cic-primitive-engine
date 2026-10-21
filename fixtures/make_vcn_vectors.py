#!/usr/bin/env python3
"""Derives conformance/differential/verdict/vcn_<case>/input.json from the vendored fixture fixtures/vcn-v1.

    make_vcn_vectors.py            write the input.json files
    make_vcn_vectors.py --check    fail if they differ, if the manifest does not match, or if a pinned expected.json
                                   disagrees with the fixture's per-path verdicts

Only stage 5 is exercised: the intent and the observed object are the fixture's registry-named files, the observation list is its
coverage, the plan is its plan.json. The digests and the canonical string in expected.json are what the engine computes; this
script never writes them, it only checks that the per-path verdicts and the object verdict in them are the fixture's.
"""
import hashlib
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FX = os.path.join(HERE, "vcn-v1")
OUT = os.path.join(HERE, "..", "conformance", "differential", "verdict")


def load(*p):
    with open(os.path.join(FX, *p), encoding="utf-8") as fh:
        return json.load(fh)


def dump(doc):
    return json.dumps(doc, indent=2, ensure_ascii=False) + "\n"


def vector_input(case):
    intent = load("cases", case, "intent.registry.json")
    obs = load("cases", case, "expected", "observed.registry.json")
    return {
        "intent": intent,
        "observed": obs["config"],
        "observation": [{"path": p, "coverage": c} for p, c in sorted(obs["coverage"].items())],
        "plan": load("plan.json"),
    }


def main(argv):
    bad = []
    with open(os.path.join(FX, "MANIFEST.sha256"), encoding="utf-8") as fh:
        for line in fh.read().splitlines():
            digest, name = line.split("  ", 1)
            with open(os.path.join(FX, name), "rb") as f:
                if hashlib.sha256(f.read()).hexdigest() != digest:
                    bad.append("manifest: " + name)
    for case in sorted(load("cases.json")):
        d = os.path.join(OUT, "vcn_" + case.replace("-", "_"))
        want = dump(vector_input(case))
        path = os.path.join(d, "input.json")
        if "--check" in argv:
            if not os.path.exists(path) or open(path, encoding="utf-8").read() != want:
                bad.append(path)
            ex = os.path.join(d, "expected.json")
            if os.path.exists(ex):
                with open(ex, encoding="utf-8") as fh:
                    got = json.load(fh)["verdict"]
                fx = load("cases", case, "expected", "verdict.fields.json")
                if got["fields"] != fx["fields"] or got["object"] != fx["object"]:
                    bad.append("verdict differs from the fixture: " + case)
            else:
                bad.append("missing " + ex)
        else:
            os.makedirs(d, exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(want)
    if bad:
        print("\n".join(bad))
        return 1
    print("ok" if "--check" in argv else "written")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
