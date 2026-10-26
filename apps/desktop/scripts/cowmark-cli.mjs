// Builds the `cowmark` command (crates/cowmark-cli) and copies it to
// src-tauri/binaries/cowmark-<target triple>, where tauri.catercow.conf.json's
// bundle.externalBin expects it. The target is the first argument, else the one
// `tauri build` is building for (TAURI_ENV_TARGET_TRIPLE), else this machine's.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const desktopRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const crate = join(desktopRoot, "..", "..", "crates", "cowmark-cli");

function hostTriple() {
  const output = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const host = /^host:\s*(\S+)/m.exec(output)?.[1];
  if (!host) throw new Error("Couldn't read the host target from `rustc -vV`.");
  return host;
}

const target = process.argv[2] || process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple();
const exe = target.includes("windows") ? ".exe" : "";

execFileSync(
  "cargo",
  ["build", "--release", "--locked", "--manifest-path", join(crate, "Cargo.toml"), "--target", target],
  { stdio: "inherit" }
);

const binaries = join(desktopRoot, "src-tauri", "binaries");
mkdirSync(binaries, { recursive: true });
const destination = join(binaries, `cowmark-${target}${exe}`);
copyFileSync(join(crate, "target", target, "release", `cowmark${exe}`), destination);
console.log(`cowmark → ${destination}`);
