//! Integration tests for wasm-bodge packaging
//!
//! These tests verify that the generated npm package works correctly
//! across all supported JavaScript environments.
//!
//! Test structure:
//! - tests/fixtures/test-crate/  - A minimal wasm-bindgen Rust crate
//! - tests/templates/            - Self-contained test projects for each environment
//!
//! Browser-based tests (webpack, vite, iife) use a Rust HTTP server + Puppeteer
//! to verify the code actually works in a real browser environment.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;

static BUILD_RESULT: OnceLock<Result<PathBuf, String>> = OnceLock::new();
static PUPPETEER_INSTALLED: OnceLock<Result<(), String>> = OnceLock::new();

/// Build the test fixture once and return the path to the built package
fn get_test_package() -> Result<PathBuf> {
    let result = BUILD_RESULT.get_or_init(build_test_package);

    match result {
        Ok(path) => Ok(path.clone()),
        Err(e) => anyhow::bail!("Test package build failed: {}", e),
    }
}

const TEST_PACKAGE_JSON: &str = r#"{
  "name": "test-wasm-lib",
  "version": "0.1.0",
  "license": "MIT",
  "description": "Test fixture for wasm-bodge"
}
"#;

/// Copy the test fixture crate's source files to a destination directory,
/// excluding build artifacts (dist/, target/).
fn copy_fixture_crate(dest: &Path) -> Result<(), String> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = project_root.join("tests/fixtures/test-crate");

    std::fs::create_dir_all(dest.join("src"))
        .map_err(|e| format!("Failed to create crate dirs: {}", e))?;
    for file in &["Cargo.toml", "Cargo.lock", "src/lib.rs"] {
        std::fs::copy(fixture.join(file), dest.join(file))
            .map_err(|e| format!("Failed to copy {}: {}", file, e))?;
    }
    Ok(())
}

fn build_test_package() -> Result<PathBuf, String> {
    // Copy fixture to a temp directory so we don't modify the repo
    let crate_path = std::env::temp_dir().join("wasm-bodge-test-build");
    let _ = std::fs::remove_dir_all(&crate_path);
    copy_fixture_crate(&crate_path)?;

    let package_json = crate_path.join("package.json");
    let out_dir = crate_path.join("dist");

    std::fs::write(&package_json, TEST_PACKAGE_JSON)
        .map_err(|e| format!("Failed to write package.json: {e}"))?;

    // Build with a debug variant so the debug-symbol and ./debug export
    // tests can run against the same cached build.
    let output = run_wasm_bodge_build(
        &crate_path,
        &package_json,
        &out_dir,
        &["--debug-profile", "wasm-debug"],
    );

    if !output.status.success() {
        return Err(format!(
            "wasm-bodge build failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ));
    }

    // Return the crate_path (where package.json lives), not out_dir
    Ok(crate_path)
}

/// Install puppeteer once in tests/puppeteer_runner/
fn ensure_puppeteer_installed() -> Result<()> {
    let result = PUPPETEER_INSTALLED.get_or_init(|| {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let runner_dir = project_root.join("tests/puppeteer_runner");

        // Check if node_modules exists with puppeteer
        let puppeteer_path = runner_dir.join("node_modules/puppeteer");
        if puppeteer_path.exists() {
            return Ok(());
        }

        println!("Installing puppeteer...");
        let output = Command::new("npm")
            .args(["install"])
            .current_dir(&runner_dir)
            .output()
            .map_err(|e| format!("Failed to run npm install: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "npm install failed in tests/puppeteer_runner/: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    });

    match result {
        Ok(()) => Ok(()),
        Err(e) => anyhow::bail!("Puppeteer installation failed: {}", e),
    }
}

/// Browser test configuration
#[derive(Debug, Clone, Copy)]
enum BrowserTestKind {
    /// Serve static files from dist/ after webpack build
    StaticDist,
    /// Run vite dev server
    ViteDev,
    /// Build with vite, then serve with vite preview
    ViteBuild,
    /// Serve static files from test dir (for IIFE)
    StaticRoot,
}

/// Determine the browser test kind for a template, if any
fn browser_test_kind(template_name: &str) -> Option<BrowserTestKind> {
    if template_name.starts_with("webpack_") {
        Some(BrowserTestKind::StaticDist)
    } else if template_name.starts_with("vite_dev_") {
        Some(BrowserTestKind::ViteDev)
    } else if template_name.starts_with("vite_build_") {
        Some(BrowserTestKind::ViteBuild)
    } else if template_name == "iife_script" {
        Some(BrowserTestKind::StaticRoot)
    } else {
        None
    }
}

/// Run a test for the given template directory name
fn run_test(template_name: &str) -> Result<()> {
    let package_dir = get_test_package()?;

    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let template_dir = project_root.join("tests/templates").join(template_name);

    if !template_dir.exists() {
        anyhow::bail!("Template directory not found: {}", template_dir.display());
    }

    // Create a temporary directory for this test
    let temp_dir = std::env::temp_dir().join(format!("wasm-bodge-test-{}", template_name));

    // Clean up any previous run
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Copy template files to temp directory
    copy_dir_recursive(&template_dir, &temp_dir)?;

    // Install the package being tested
    install_package(&temp_dir, &package_dir)?;

    // Check if template has devDependencies (needs npm install)
    if has_dev_dependencies(&temp_dir)? {
        run_npm_command(&temp_dir, &["install"])?;
    }

    // Run build
    run_npm_command(&temp_dir, &["run", "build"])?;

    // Run test - either browser test or npm test
    if let Some(kind) = browser_test_kind(template_name) {
        run_browser_test(&project_root, &temp_dir, kind)?;
    } else {
        run_npm_command(&temp_dir, &["test"])?;
    }

    // Cleanup on success
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            std::fs::copy(&src_path, &dest_path)?;
        }
    }

    Ok(())
}

fn install_package(temp_dir: &Path, package_dir: &Path) -> Result<()> {
    // Create tarball from package
    let output = Command::new("npm")
        .args(["pack", "--pack-destination", &temp_dir.to_string_lossy()])
        .current_dir(package_dir)
        .output()
        .context("Failed to run npm pack")?;

    if !output.status.success() {
        anyhow::bail!(
            "npm pack failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Find the tarball (npm pack outputs the filename)
    let tarball_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let actual_tarball = temp_dir.join(&tarball_name);

    // Install it
    let output = Command::new("npm")
        .args(["install", &actual_tarball.to_string_lossy()])
        .current_dir(temp_dir)
        .output()
        .context("Failed to run npm install")?;

    if !output.status.success() {
        anyhow::bail!(
            "npm install failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

fn has_dev_dependencies(dir: &Path) -> Result<bool> {
    let package_json_path = dir.join("package.json");
    let content = std::fs::read_to_string(&package_json_path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    Ok(json.get("devDependencies").is_some())
}

fn run_npm_command(dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("npm")
        .args(args)
        .current_dir(dir)
        .output()
        .context(format!("Failed to run npm {}", args.join(" ")))?;

    if !output.status.success() {
        anyhow::bail!(
            "npm {} failed:\nstdout: {}\nstderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

// ============================================================================
// Browser testing with Rust HTTP server + Puppeteer
// ============================================================================

fn run_browser_test(project_root: &Path, test_dir: &Path, kind: BrowserTestKind) -> Result<()> {
    ensure_puppeteer_installed()?;

    match kind {
        BrowserTestKind::StaticDist => {
            // Serve dist/ directory with our Rust server
            let serve_dir = test_dir.join("dist");
            run_static_server_test(project_root, &serve_dir, "/index.html")?;
        }
        BrowserTestKind::StaticRoot => {
            // For IIFE: copy the IIFE bundle to test dir, then serve
            let iife_src = test_dir.join("node_modules/test-wasm-lib/dist/iife/index.js");
            let iife_dest = test_dir.join("test-wasm-lib-iife.js");
            std::fs::copy(&iife_src, &iife_dest).context("Failed to copy IIFE bundle")?;
            run_static_server_test(project_root, test_dir, "/index.html")?;
        }
        BrowserTestKind::ViteDev => {
            run_vite_dev_test(project_root, test_dir)?;
        }
        BrowserTestKind::ViteBuild => {
            run_vite_build_test(project_root, test_dir)?;
        }
    }

    Ok(())
}

/// Start a static file server, run puppeteer, then shut down the server
fn run_static_server_test(project_root: &Path, serve_dir: &Path, path: &str) -> Result<()> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use tiny_http::{Response, Server};

    // Find a free port
    let server = Server::http("127.0.0.1:0")
        .map_err(|e| anyhow::anyhow!("Failed to start HTTP server: {}", e))?;
    let port = server.server_addr().to_ip().map(|a| a.port()).unwrap_or(0);
    let url = format!("http://127.0.0.1:{}{}", port, path);

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let serve_dir = serve_dir.to_path_buf();

    // Spawn server thread
    let server_handle = thread::spawn(move || {
        while !shutdown_clone.load(Ordering::Relaxed) {
            // Use a short timeout so we can check the shutdown flag
            if let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_millis(100)) {
                let url_path = request.url().to_string();
                let file_path = if url_path == "/" {
                    serve_dir.join("index.html")
                } else {
                    serve_dir.join(url_path.trim_start_matches('/'))
                };

                if file_path.exists() && file_path.is_file() {
                    let content = std::fs::read(&file_path).unwrap_or_default();
                    let content_type = guess_content_type(&file_path);
                    let response = Response::from_data(content).with_header(
                        tiny_http::Header::from_bytes("Content-Type", content_type).unwrap(),
                    );
                    let _ = request.respond(response);
                } else {
                    let _ =
                        request.respond(Response::from_string("Not Found").with_status_code(404));
                }
            }
        }
    });

    // Run puppeteer
    let result = run_puppeteer_check(project_root, &url);

    // Shutdown server
    shutdown.store(true, Ordering::Relaxed);
    let _ = server_handle.join();

    result
}

/// Run vite dev server and test with puppeteer
fn run_vite_dev_test(project_root: &Path, test_dir: &Path) -> Result<()> {
    // Start vite dev server (let it pick default port, we'll parse output)
    let mut vite = Command::new("npx")
        .args(["vite"])
        .current_dir(test_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start vite dev server")?;

    // Wait for server to be ready and extract URL
    let result = wait_for_vite_and_test(project_root, &mut vite);

    // Kill vite
    let _ = vite.kill();
    let _ = vite.wait();

    result
}

/// Build with vite, then run vite preview and test
fn run_vite_build_test(project_root: &Path, test_dir: &Path) -> Result<()> {
    // vite build already ran as part of npm run build

    // Verify the @vite-ignore fix worked - there should be at most one .wasm file
    // Multiple .wasm files means vite's asset processor duplicated the wasm
    let assets_dir = test_dir.join("dist/assets");
    if assets_dir.exists() {
        let wasm_files: Vec<_> = std::fs::read_dir(&assets_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "wasm"))
            .collect();
        if wasm_files.len() > 1 {
            anyhow::bail!(
                "@vite-ignore fix failed: found {} .wasm files in dist/assets (expected at most 1)",
                wasm_files.len()
            );
        }
    }

    // Start vite preview server (let it pick default port, we'll parse output)
    let mut vite = Command::new("npx")
        .args(["vite", "preview"])
        .current_dir(test_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start vite preview server")?;

    // Wait for server to be ready and extract URL
    let result = wait_for_vite_and_test(project_root, &mut vite);

    // Kill vite
    let _ = vite.kill();
    let _ = vite.wait();

    result
}

/// Wait for vite server to output its URL, then run puppeteer
fn wait_for_vite_and_test(project_root: &Path, vite: &mut Child) -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    eprintln!("[vite] Starting to wait for vite server...");

    // Vite may output to stdout or stderr depending on environment/tty
    let stdout = vite.stdout.take();
    let stderr = vite.stderr.take();

    // Regex to strip ANSI escape codes
    let ansi_pattern = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    // Match "Local: http://..." after stripping ANSI codes
    let url_pattern = regex::Regex::new(r"Local:\s+(http://\S+)").unwrap();
    let (tx, rx) = mpsc::channel();

    // Spawn thread to read stdout
    if let Some(stdout) = stdout {
        let tx = tx.clone();
        let pattern = url_pattern.clone();
        let ansi = ansi_pattern.clone();
        thread::spawn(move || {
            eprintln!("[vite] stdout reader thread started");
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        eprintln!("[vite stdout] {}", line);
                        // Strip ANSI codes before matching
                        let clean = ansi.replace_all(&line, "");
                        if let Some(caps) = pattern.captures(&clean) {
                            let _ = tx.send(caps[1].to_string());
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("[vite stdout error] {}", e);
                        break;
                    }
                }
            }
            eprintln!("[vite] stdout reader thread ending");
        });
    } else {
        eprintln!("[vite] No stdout pipe!");
    }

    // Spawn thread to read stderr
    if let Some(stderr) = stderr {
        let tx = tx.clone();
        let pattern = url_pattern.clone();
        let ansi = ansi_pattern.clone();
        thread::spawn(move || {
            eprintln!("[vite] stderr reader thread started");
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        eprintln!("[vite stderr] {}", line);
                        // Strip ANSI codes before matching
                        let clean = ansi.replace_all(&line, "");
                        if let Some(caps) = pattern.captures(&clean) {
                            let _ = tx.send(caps[1].to_string());
                            break;
                        }
                    }
                    Err(e) => {
                        eprintln!("[vite stderr error] {}", e);
                        break;
                    }
                }
            }
            eprintln!("[vite] stderr reader thread ending");
        });
    } else {
        eprintln!("[vite] No stderr pipe!");
    }

    // Wait for URL with timeout
    eprintln!("[vite] Waiting for URL (30s timeout)...");
    let url = rx
        .recv_timeout(Duration::from_secs(30))
        .context("Timeout waiting for vite server URL")?;

    eprintln!("[vite] Got URL: {}", url);
    run_puppeteer_check(project_root, &url)
}

/// Run the puppeteer check script
fn run_puppeteer_check(project_root: &Path, url: &str) -> Result<()> {
    let runner_dir = project_root.join("tests/puppeteer_runner");
    let check_script = runner_dir.join("check.mjs");

    let output = Command::new("node")
        .args([check_script.to_str().unwrap(), url])
        .current_dir(&runner_dir)
        .output()
        .context("Failed to run puppeteer check")?;

    if !output.status.success() {
        anyhow::bail!(
            "Puppeteer test failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(())
}

/// Guess content type from file extension
fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

// ============================================================================
// Individual test functions - one per environment
// These are separate so they can run in parallel and failures are clear
// ============================================================================

#[test]
fn test_node_esm_fullfat() {
    run_test("node_esm_fullfat").unwrap();
}

#[test]
fn test_node_esm_slim() {
    run_test("node_esm_slim").unwrap();
}

#[test]
fn test_node_cjs_fullfat() {
    run_test("node_cjs_fullfat").unwrap();
}

#[test]
fn test_node_cjs_slim() {
    run_test("node_cjs_slim").unwrap();
}

#[test]
fn test_webpack_esm_fullfat() {
    run_test("webpack_esm_fullfat").unwrap();
}

#[test]
fn test_webpack_esm_slim() {
    run_test("webpack_esm_slim").unwrap();
}

#[test]
fn test_webpack_cjs_fullfat() {
    run_test("webpack_cjs_fullfat").unwrap();
}

#[test]
fn test_webpack_cjs_slim() {
    run_test("webpack_cjs_slim").unwrap();
}

#[test]
fn test_vite_dev_fullfat() {
    run_test("vite_dev_fullfat").unwrap();
}

#[test]
fn test_vite_dev_slim() {
    run_test("vite_dev_slim").unwrap();
}

#[test]
fn test_vite_build_fullfat() {
    run_test("vite_build_fullfat").unwrap();
}

#[test]
fn test_vite_build_slim() {
    run_test("vite_build_slim").unwrap();
}

#[test]
fn test_vite_build_slim_debug() {
    run_test("vite_build_slim_debug").unwrap();
}

#[test]
fn test_workerd_fullfat() {
    run_test("workerd_fullfat").unwrap();
}

#[test]
fn test_workerd_slim() {
    run_test("workerd_slim").unwrap();
}

#[test]
fn test_node_esm_cross_init() {
    run_test("node_esm_cross_init").unwrap();
}

#[test]
fn test_node_cjs_cross_init() {
    run_test("node_cjs_cross_init").unwrap();
}

#[test]
fn test_iife_script() {
    run_test("iife_script").unwrap();
}

/// Parse wasm custom sections and check whether any section name begins with
/// `.debug_` (DWARF debug info). Returns an error if the file isn't a valid
/// wasm binary.
fn has_debug_sections(path: &Path) -> Result<bool> {
    let bytes = std::fs::read(path).context("Failed to read wasm file")?;
    if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
        anyhow::bail!("Not a valid wasm file: {}", path.display());
    }

    // Read an LEB128-encoded unsigned integer. Returns (value, bytes_consumed).
    fn read_leb128(buf: &[u8]) -> Result<(u64, usize)> {
        let mut result: u64 = 0;
        let mut shift = 0;
        let mut idx = 0;
        loop {
            if idx >= buf.len() {
                anyhow::bail!("Unexpected end of LEB128");
            }
            let byte = buf[idx];
            idx += 1;
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok((result, idx));
            }
            shift += 7;
            if shift >= 64 {
                anyhow::bail!("LEB128 too long");
            }
        }
    }

    let mut pos = 8;
    while pos < bytes.len() {
        let section_id = bytes[pos];
        pos += 1;
        let (section_size, size_len) = read_leb128(&bytes[pos..])?;
        pos += size_len;
        let section_end = pos + section_size as usize;
        if section_end > bytes.len() {
            anyhow::bail!("Section extends past end of file");
        }

        if section_id == 0 {
            // Custom section: first field is the UTF-8 name
            let (name_len, name_len_bytes) = read_leb128(&bytes[pos..section_end])?;
            let name_start = pos + name_len_bytes;
            let name_end = name_start + name_len as usize;
            if name_end > section_end {
                anyhow::bail!("Custom section name extends past section");
            }
            let name = std::str::from_utf8(&bytes[name_start..name_end])
                .context("Custom section name is not valid UTF-8")?;
            if name.starts_with(".debug_") {
                return Ok(true);
            }
        }

        pos = section_end;
    }

    Ok(false)
}

/// Verify the normal wasm has no debug symbols and the debug wasm does.
#[test]
fn test_debug_symbols() {
    let package_dir = get_test_package().unwrap();
    let dist = package_dir.join("dist");

    let normal = dist.join("test-wasm-lib.wasm");
    let debug = dist.join("test-wasm-lib-debug.wasm");

    assert!(normal.exists(), "normal wasm missing: {}", normal.display());
    assert!(debug.exists(), "debug wasm missing: {}", debug.display());

    assert!(
        !has_debug_sections(&normal).unwrap(),
        "normal wasm should have no .debug_* sections (stripped by wasm-opt)"
    );
    assert!(
        has_debug_sections(&debug).unwrap(),
        "debug wasm should have .debug_* sections (preserved by the dedicated debug profile)"
    );
}

#[test]
fn test_node_esm_debug() {
    run_test("node_esm_debug").unwrap();
}

#[test]
fn test_node_esm_slim_debug() {
    run_test("node_esm_slim_debug").unwrap();
}

#[test]
fn test_node_cjs_slim_debug() {
    run_test("node_cjs_slim_debug").unwrap();
}

#[test]
fn test_webpack_esm_slim_debug() {
    run_test("webpack_esm_slim_debug").unwrap();
}

#[test]
fn test_webpack_cjs_slim_debug() {
    run_test("webpack_cjs_slim_debug").unwrap();
}

#[test]
fn test_vite_dev_slim_debug() {
    run_test("vite_dev_slim_debug").unwrap();
}

#[test]
fn test_workerd_slim_debug() {
    run_test("workerd_slim_debug").unwrap();
}

/// Test that building with a scoped npm package name (e.g. @scope/name) works.
#[test]
fn test_scoped_package_name() {
    let crate_copy = std::env::temp_dir().join("wasm-bodge-test-scoped");
    let _ = std::fs::remove_dir_all(&crate_copy);
    copy_fixture_crate(&crate_copy).unwrap();

    // Write a scoped package.json
    let package_json = crate_copy.join("package.json");
    std::fs::write(
        &package_json,
        r#"{
  "name": "@test-scope/test-wasm-lib",
  "version": "0.1.0",
  "license": "MIT",
  "description": "Test fixture for wasm-bodge"
}
"#,
    )
    .unwrap();

    let out_dir = crate_copy.join("dist");
    let output = run_wasm_bodge_build(&crate_copy, &package_json, &out_dir, &[]);

    assert!(
        output.status.success(),
        "wasm-bodge build failed with status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // Verify key output files exist
    assert!(out_dir.join("index.d.ts").exists(), "index.d.ts missing");
    assert!(out_dir.join("esm/node.js").exists(), "esm/node.js missing");
    assert!(
        out_dir.join("cjs/node.cjs").exists(),
        "cjs/node.cjs missing"
    );
    assert!(
        out_dir.join("test-wasm-lib.wasm").exists(),
        ".wasm file missing"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&crate_copy);
}

fn run_wasm_bodge_build(
    crate_path: &Path,
    package_json: &Path,
    out_dir: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut args = vec![
        "run",
        "--release",
        "--",
        "build",
        "--crate-path",
        crate_path.to_str().unwrap(),
        "--package-json",
        package_json.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
    ];
    args.extend(extra_args);

    Command::new("cargo")
        .args(&args)
        .current_dir(&project_root)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("Failed to run cargo")
}

fn write_test_package_json(path: &Path) {
    std::fs::write(path, TEST_PACKAGE_JSON).expect("Failed to write package.json");
}

/// With `[profile.wasm-debug]` declared, the debug variant is compiled by a
/// dedicated cargo build (not copied from the release wasm).
#[test]
fn test_two_profile_debug_build() {
    let crate_path = std::env::temp_dir().join("wasm-bodge-test-two-profile");
    let _ = std::fs::remove_dir_all(&crate_path);
    copy_fixture_crate(&crate_path).unwrap();

    let package_json = crate_path.join("package.json");
    write_test_package_json(&package_json);
    let out_dir = crate_path.join("dist");

    let output = run_wasm_bodge_build(
        &crate_path,
        &package_json,
        &out_dir,
        &["--debug-profile", "wasm-debug"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "wasm-bodge build failed:\nstdout: {stdout}\nstderr: {stderr}",
    );

    let debug_artifact =
        crate_path.join("target/wasm32-unknown-unknown/wasm-debug/test_wasm_lib.wasm");
    let debug_artifact_display = debug_artifact.display();
    assert!(
        debug_artifact.exists(),
        "expected debug-profile artifact at {debug_artifact_display}",
    );

    let release_wasm = out_dir.join("test-wasm-lib.wasm");
    let debug_wasm = out_dir.join("test-wasm-lib-debug.wasm");
    assert!(
        !has_debug_sections(&release_wasm).unwrap(),
        "release wasm should have no DWARF"
    );
    assert!(
        has_debug_sections(&debug_wasm).unwrap(),
        "debug wasm should have DWARF"
    );

    let _ = std::fs::remove_dir_all(&crate_path);
}

/// Passing `--debug-profile <name>` where `[profile.<name>]` is not declared
/// fails with a wrapped error pointing the user at the required snippet.
#[test]
fn test_missing_profile_is_hard_error() {
    let crate_path = std::env::temp_dir().join("wasm-bodge-test-missing-profile");
    let _ = std::fs::remove_dir_all(&crate_path);
    copy_fixture_crate(&crate_path).unwrap();

    let package_json = crate_path.join("package.json");
    write_test_package_json(&package_json);
    let out_dir = crate_path.join("dist");

    // Pass a profile name that is guaranteed not to exist in the fixture's
    // Cargo.toml so the build hits the missing-profile hard-error path.
    let missing_profile = "definitely-not-declared";
    let output = run_wasm_bodge_build(
        &crate_path,
        &package_json,
        &out_dir,
        &["--debug-profile", missing_profile],
    );

    assert!(
        !output.status.success(),
        "wasm-bodge should fail when [profile.{missing_profile}] is missing"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    let expected =
        format!("--debug-profile {missing_profile} requires a [profile.{missing_profile}]");
    assert!(
        stderr.contains(&expected),
        "expected wasm-bodge-branded error mentioning --debug-profile and \
         [profile.{missing_profile}], got:\n{stderr}",
    );

    let _ = std::fs::remove_dir_all(&crate_path);
}

/// `--debug-profile <name>` drives the build with the named profile.
#[test]
fn test_custom_debug_profile_name() {
    let crate_path = std::env::temp_dir().join("wasm-bodge-test-custom-profile");
    let _ = std::fs::remove_dir_all(&crate_path);
    copy_fixture_crate(&crate_path).unwrap();

    let cargo_toml = crate_path.join("Cargo.toml");
    let existing = std::fs::read_to_string(&cargo_toml).unwrap();
    std::fs::write(
        &cargo_toml,
        format!("{existing}\n\n[profile.my-weird-debug]\ninherits = \"dev\"\ndebug = \"full\"\n",),
    )
    .unwrap();

    let package_json = crate_path.join("package.json");
    write_test_package_json(&package_json);
    let out_dir = crate_path.join("dist");

    let output = run_wasm_bodge_build(
        &crate_path,
        &package_json,
        &out_dir,
        &["--debug-profile", "my-weird-debug"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "build failed:\nstdout: {stdout}\nstderr: {stderr}",
    );

    let artifact =
        crate_path.join("target/wasm32-unknown-unknown/my-weird-debug/test_wasm_lib.wasm");
    let artifact_display = artifact.display();
    assert!(
        artifact.exists(),
        "expected custom-profile artifact at {artifact_display}",
    );

    let _ = std::fs::remove_dir_all(&crate_path);
}

/// `--debug-profile release` reuses the release profile for the debug
/// variant (v0.2.3 migration path).
#[test]
fn test_debug_profile_release_migration() {
    let crate_path = std::env::temp_dir().join("wasm-bodge-test-release-migration");
    let _ = std::fs::remove_dir_all(&crate_path);
    copy_fixture_crate(&crate_path).unwrap();

    let package_json = crate_path.join("package.json");
    write_test_package_json(&package_json);
    let out_dir = crate_path.join("dist");

    let output = run_wasm_bodge_build(
        &crate_path,
        &package_json,
        &out_dir,
        &["--debug-profile", "release"],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "build failed:\nstdout: {stdout}\nstderr: {stderr}",
    );

    let debug_wasm = out_dir.join("test-wasm-lib-debug.wasm");
    assert!(debug_wasm.exists(), "debug wasm missing");
    assert!(
        has_debug_sections(&debug_wasm).unwrap(),
        "debug wasm should have DWARF (inherited from [profile.release] debug=true)"
    );

    let _ = std::fs::remove_dir_all(&crate_path);
}
