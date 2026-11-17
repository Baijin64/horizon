"""Exercise the wrapper's runtime completion/report contract without game resources."""
from contextlib import ExitStack
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from tools import test_level_environment_reads_parity as parity


class LevelEnvironmentReadsRunnerTests(unittest.TestCase):
    def test_report_requires_exactly_six_successful_resource_tests(self):
        with tempfile.TemporaryDirectory() as temp, ExitStack() as patches:
            root = Path(temp).resolve()
            source = root / "tools/test_level_environment_reads_parity.py"
            for name in (
                "tools/test_level_environment_reads_parity.py", "Cargo.toml", "Cargo.lock",
                "tools/oracle_harness.py", "tools/test_rust_parity.py", "tools/lab.py",
                "tools/integration_env.py",
                "mods/horizon/src/test/java/dev/horizonlab/parity/VanillaLevelEnvironmentReadsOracle.java",
                "mods/horizon/src/test/java/dev/horizonlab/parity/ParityData.java",
            ):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("unit-test input\n")
            fixture = root / "tests/fixtures/level_environment_reads.jsonl"
            fixture.parent.mkdir(parents=True)
            fixture.write_text('{"kind":"biome","name":"example","position":[0,64,0]}\n')
            expected = '{"name":"example","value":"minecraft:plains"}\n'
            fixture.with_name("level_environment_reads_expected.jsonl").write_text(expected)
            executable = root / "rust-tests"
            executable.write_text("controlled test executable\n")
            jars = {side: root / f"{side}.jar" for side in ("server", "client")}
            for path in jars.values():
                path.write_text("unit-test artifact\n")
            lock = {"minecraft": {"version": "26.3", "java_major": 25}}
            build = root / ".local/level-environment-reads"
            report = build / "report.json"

            def compile_rust(command, directory, tag, **kwargs):
                artifact = {"reason": "compiler-artifact", "executable": str(executable),
                            "profile": {"test": True},
                            "target": {"name": "horizon_client", "kind": ["lib"]}}
                (directory / f"{tag}.log").write_text(json.dumps(artifact) + "\n")

            def run_java(directory, classes, dependencies, oracle, arguments, side):
                arguments[1].write_text(expected)

            def run_rust(command, *, stdout, check, **kwargs):
                stdout.write(text)
                if returncode and check:
                    raise subprocess.CalledProcessError(returncode, command)
                return subprocess.CompletedProcess(command, returncode)

            patches.enter_context(mock.patch.object(parity, "__file__", str(source)))
            for name, value in (("ROOT", root), ("LOCAL", root / ".local"),
                                ("RESOURCES", root / "resources")):
                patches.enter_context(mock.patch.object(parity.lab, name, value))
            patches.enter_context(mock.patch.object(parity.base, "verify_artifacts",
                                                    return_value=(lock, jars, [])))
            patches.enter_context(mock.patch.object(parity.harness, "compile_oracle",
                                                    return_value=(root / "classes", None, None)))
            patches.enter_context(mock.patch.object(parity.harness, "java_run", side_effect=run_java))
            patches.enter_context(mock.patch.object(parity.harness, "run_logged", side_effect=compile_rust))
            patches.enter_context(mock.patch.object(parity.subprocess, "run", side_effect=run_rust))
            patches.enter_context(mock.patch("builtins.print"))

            for summary, returncode, error in (
                ("ok. 6 passed; 0 failed; 0 ignored;", 0, None),
                ("ok. 0 passed; 0 failed; 0 ignored;", 0, AssertionError),
                ("ok. 5 passed; 0 failed; 0 ignored;", 0, AssertionError),
                ("ok. 7 passed; 0 failed; 0 ignored;", 0, AssertionError),
                ("ok. 5 passed; 0 failed; 1 ignored;", 0, AssertionError),
                ("ok. 6 passed; 0 failed; 1 ignored;", 0, AssertionError),
                ("ok. 6 passed; 1 failed; 0 ignored;", 0, AssertionError),
                ("FAILED. 5 passed; 1 failed; 0 ignored;", 101, subprocess.CalledProcessError),
                ("ok. 6 passed; 0 failed; 0 ignored;", 101, subprocess.CalledProcessError),
                (None, 0, AssertionError),
            ):
                text = f"test result: {summary} 0 measured; 5162 filtered out; finished in 0.1s\n" \
                    if summary is not None else "running 6 tests\n"
                with self.subTest(summary=summary, returncode=returncode):
                    build.mkdir(parents=True, exist_ok=True)
                    report.write_text('{"previous_success":true}\n')
                    if error is not None:
                        with self.assertRaises(error):
                            parity.main()
                        self.assertFalse(report.exists(), "an incomplete run must not report success")
                    else:
                        parity.main()
                        values = json.loads(report.read_text())
                        self.assertEqual(values["resource_tests"], 6)
                        self.assertEqual((values["cases"], values["fixed_cases"]), (1, 1))


if __name__ == "__main__":
    unittest.main()
