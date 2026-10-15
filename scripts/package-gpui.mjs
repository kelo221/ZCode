#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
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
  const platform =
    process.platform === "win32" ? "windows" : process.platform === "darwin" ? "darwin" : "linux";
  const arch =
    process.arch === "x64" ? "x86_64" : process.arch === "arm64" ? "aarch64" : process.arch;
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

  // Browser Use: the native messaging host must sit next to the executable
  // (Settings › Browser Use registers that path), and the unpacked extension
  // goes into `browser-extension/` for Load unpacked. The extension build needs bun.
  const hostName = platform === "windows" ? "zcode-browser-host.exe" : "zcode-browser-host";
  await cp(
    resolve(root, "apps", "zcode-gpui", "target", "release", hostName),
    join(defaultOutDir, hostName),
  );
  if (!process.argv.includes("--skip-extension")) {
    const extensionRes = spawnSync(
      "bun",
      [
        resolve(root, "apps", "zcode-browser-extension", "scripts", "build.ts"),
        "--outdir",
        join(defaultOutDir, "browser-extension"),
      ],
      { stdio: "inherit", cwd: resolve(root, "apps", "zcode-browser-extension") },
    );
    if (extensionRes.status !== 0) {
      console.error(
        "Browser extension build failed (pass --skip-extension to package without it).",
      );
      process.exit(extensionRes.status || 1);
    }
  }

  // Optional: the compiled CLI backend (about 150 MiB) beside the binary, which GPUI
  // prefers over a repository checkout or the installed Desktop runtime. Needs bun.
  if (process.argv.includes("--with-backend")) {
    const backendRes = spawnSync(
      "bun",
      [
        resolve(root, "apps", "zcode-gpui", "scripts", "build_backend.ts"),
        "--outdir",
        defaultOutDir,
      ],
      { stdio: "inherit", cwd: root },
    );
    if (backendRes.status !== 0) {
      console.error("Backend build failed.");
      process.exit(backendRes.status || 1);
    }
  }

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
    "utf-8",
  );

  console.log(`Release package ready at: ${defaultOutDir}`);
  console.log(`SHA-256: ${hash}`);
}

main().catch((err) => {
  console.error("Packaging failed:", err);
  process.exit(1);
});
