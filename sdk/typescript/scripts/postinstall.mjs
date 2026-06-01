// Post-install: download the pre-built aipdf binary for this platform.
// Runs automatically via npm's `postinstall` lifecycle hook.
// On failure prints a warning and exits 0 so `npm install` still succeeds.
import { execSync } from "node:child_process";
import { createWriteStream, existsSync, chmodSync, unlinkSync } from "node:fs";
import { mkdirSync } from "node:fs";
import { get } from "node:https";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const REPO = "https://github.com/tanmaysheoran/ai.pdf";

const PKG_ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const BIN_DIR = join(PKG_ROOT, "bin");

function detectArtifact() {
  const { platform, arch } = process;
  if (platform === "linux"  && arch === "x64")   return "aipdf-x86_64-linux.tar.gz";
  if (platform === "linux"  && arch === "arm64")  return "aipdf-aarch64-linux.tar.gz";
  if (platform === "darwin" && arch === "x64")    return "aipdf-x86_64-macos.tar.gz";
  if (platform === "darwin" && arch === "arm64")  return "aipdf-aarch64-macos.tar.gz";
  if (platform === "win32"  && arch === "x64")    return "aipdf-x86_64-windows.zip";
  return null;
}

// Download `url` to `dest`, following up to 5 redirects.
function download(url, dest, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);
    const cleanup = (err) => { file.destroy(); try { unlinkSync(dest); } catch {} reject(err); };

    get(url, (res) => {
      if ((res.statusCode === 301 || res.statusCode === 302) && res.headers.location) {
        file.close();
        try { unlinkSync(dest); } catch {}
        if (redirectsLeft <= 0) return reject(new Error("too many redirects"));
        return download(res.headers.location, dest, redirectsLeft - 1).then(resolve, reject);
      }
      if (res.statusCode !== 200) return cleanup(new Error(`HTTP ${res.statusCode} fetching ${url}`));
      res.pipe(file);
      file.on("finish", () => file.close(resolve));
      file.on("error", cleanup);
      res.on("error", cleanup);
    }).on("error", cleanup);
  });
}

async function main() {
  const artifact = detectArtifact();
  if (!artifact) {
    console.warn(
      `aipdf: no pre-built binary for ${process.platform}/${process.arch} — ` +
      `skipping auto-install. Set AIPDF_BIN or run: cargo install --git ${REPO} --bin aipdf aipdf-cli`,
    );
    return;
  }

  const binName = process.platform === "win32" ? "aipdf.exe" : "aipdf";
  const binPath = join(BIN_DIR, binName);

  if (existsSync(binPath)) {
    console.log(`aipdf: binary already present (${binPath})`);
    return;
  }

  mkdirSync(BIN_DIR, { recursive: true });

  const url = `${REPO}/releases/latest/download/${artifact}`;
  const tmpPath = join(BIN_DIR, artifact);

  console.log(`aipdf: downloading ${artifact}...`);
  try {
    await download(url, tmpPath);

    if (artifact.endsWith(".tar.gz")) {
      // Extract only the `aipdf` binary from the archive root.
      execSync(`tar -xzf "${tmpPath}" -C "${BIN_DIR}" aipdf`, { stdio: "pipe" });
    } else {
      // Windows .zip — use PowerShell Expand-Archive (available everywhere on Win10+).
      execSync(
        `powershell -NoProfile -Command "Expand-Archive -LiteralPath '${tmpPath}' -DestinationPath '${BIN_DIR}' -Force"`,
        { stdio: "pipe" },
      );
    }

    try { unlinkSync(tmpPath); } catch {}

    if (process.platform !== "win32") chmodSync(binPath, 0o755);

    console.log(`aipdf: installed → ${binPath}`);
  } catch (err) {
    try { unlinkSync(tmpPath); } catch {}
    console.warn(
      `aipdf: auto-install failed (${err.message}). ` +
      `Set AIPDF_BIN to the binary path or install via: curl -fsSL ${REPO}/raw/main/install.sh | sh`,
    );
    // Exit 0 so `npm install` succeeds even without the binary (read-side still works).
  }
}

main();
