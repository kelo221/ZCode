#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";

const root = resolve(import.meta.dirname, "..");
const gpuiManifest = resolve(root, "apps", "zcode-gpui", "Cargo.toml");
const defaultOutDir = resolve(root, "dist", "zcode-gpui");

async function main() {
  const pkgJsonPath = resolve(root, "package.json");
  const pkgJson = JSON.parse(await readFile(pkgJsonPath, "utf-8"));
  const version = pkgJson.version || "3.14.3";

  console.log(`Packaging ZCode GPUI frontend v${version}...`);

  // Detect platform & arch
  const platform = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "darwin" : "linux";
  const arch = process.arch === "x64" ? "x86_64" : process.arch === "arm64" ? "aarch64" : process.arch;
  const exeName = platform === "windows" ? "zcode-gpui.exe" : "zcode-gpui";

  const targetBinPath = resolve(root, "apps", "zcode-gpui", "target", "release", exeName);

  // Build release binary if not present or requested
  if (!process.argv.includes("--skip-build")) {
    console.log("Running cargo build --release...");
    const buildRes = spawnSync("cargo", ["build", "--release", "--manifest-path", gpuiManifest], {
      stdio: "inherit",
      cwd: root,
    });
    if (buildRes.status !== 0) {
      console.error("Cargo build failed.");
      process.exit(buildRes.status || 1);
    }
  }

  // Ensure output directory exists
  await rm(defaultOutDir, { recursive: true, force: true });
  await mkdir(defaultOutDir, { recursive: true });

  // Copy binary
  const destBinPath = join(defaultOutDir, exeName);
  await cp(targetBinPath, destBinPath);
  console.log(`Copied binary to: ${destBinPath}`);

  // Copy icon if available
  const iconSrc = resolve(root, "packages", "desktop", "build", "icon.ico");
  try {
    await cp(iconSrc, join(defaultOutDir, "icon.ico"));
  } catch {
    // optional
  }

  // Compute SHA-256
  const binBuffer = await readFile(destBinPath);
  const hash = createHash("sha256").update(binBuffer).digest("hex");

  // Write release manifest
  const manifest = {
    name: "zcode-gpui",
    version,
    platform: `${platform}-${arch}`,
    executable: exeName,
    sha256: hash,
    sizeBytes: binBuffer.length,
    builtAt: new Date().toISOString(),
  };

  await writeFile(
    join(defaultOutDir, "release-manifest.json"),
    JSON.stringify(manifest, null, 2),
    "utf-8"
  );

  console.log(`Release package ready at: ${defaultOutDir}`);
  console.log(`SHA-256: ${hash}`);
}

main().catch((err) => {
  console.error("Packaging failed:", err);
  process.exit(1);
});
