//! Fixed trusted local calculator; fictional inputs only in the first integration slice.

use std::{
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{oneshot, Semaphore},
};

use crate::{Error, Result};

const LIMIT: usize = 2 * 1024 * 1024;
const METHOD: &str = "conditional-decision-2.0";
static SLOT: Semaphore = Semaphore::const_new(1);
#[cfg(all(test, windows))]
static TEST_CHILD_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExampleCase {
    Baseline,
    UnknownLoss,
    ChangedFee,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExampleRequest {
    pub case_id: ExampleCase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExampleCard {
    pub years: Vec<u8>,
    pub score: Option<()>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExampleHorizon {
    pub years: u8,
    pub end: String,
    pub remaining_a: Option<f64>,
    pub remaining_b: Option<f64>,
    pub difference: Option<f64>,
    pub target: Option<f64>,
    pub goal_met_a: Option<bool>,
    pub goal_met_b: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExampleAssessment {
    pub protocol_version: u32,
    pub engine_build_id: String,
    pub methodology_version: String,
    pub mode: String,
    pub fixture_id: String,
    pub case_id: ExampleCase,
    pub as_of: String,
    pub currency: String,
    pub input_hash: String,
    pub quantity: String,
    pub invested: String,
    pub entry_costs: String,
    pub cash_remainder: String,
    pub cards: Vec<ExampleCard>,
    pub horizons: Vec<ExampleHorizon>,
    pub reason_codes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    protocol_version: u32,
    engine_build_id: String,
    methodology_version: String,
    runtime_version: String,
    bundle_sha256: String,
    runtime_sha256: String,
    fixture_id: String,
    fictional_only: bool,
}

fn failure(code: &str) -> Error {
    Error::ConstraintViolation(code.into())
}

fn hash_file(path: &Path, limit: u64) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|_| failure("CALCULATOR_UNAVAILABLE"))?;
    if file
        .metadata()
        .map_err(|_| failure("CALCULATOR_UNAVAILABLE"))?
        .len()
        > limit
    {
        return Err(failure("CALCULATOR_PACKAGE_INVALID"));
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 16384];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| failure("CALCULATOR_UNAVAILABLE"))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn verify_package(directory: &Path) -> Result<Manifest> {
    let file = std::fs::File::open(directory.join("manifest.json"))
        .map_err(|_| failure("CALCULATOR_UNAVAILABLE"))?;
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("CALCULATOR_PACKAGE_INVALID"))?;
    if bytes.len() > 65536 {
        return Err(failure("CALCULATOR_PACKAGE_INVALID"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| failure("CALCULATOR_PACKAGE_INVALID"))?;
    let runtime = directory.join(format!("node{}", std::env::consts::EXE_SUFFIX));
    if manifest.protocol_version != 1
        || manifest.methodology_version != METHOD
        || manifest.fixture_id != "conditional-example-v1"
        || !manifest.fictional_only
        || manifest.runtime_version != "v24.19.0"
        || manifest.engine_build_id.len() != 64
        || hash_file(&directory.join("engine.mjs"), LIMIT as u64)? != manifest.bundle_sha256
        || hash_file(&runtime, 128 * 1024 * 1024)? != manifest.runtime_sha256
    {
        return Err(failure("CALCULATOR_PACKAGE_INVALID"));
    }
    Ok(manifest)
}

async fn read_bounded(stream: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| failure("CALCULATOR_IO_FAILED"))?;
    if bytes.len() > limit {
        return Err(failure("CALCULATOR_OUTPUT_LIMIT"));
    }
    Ok(bytes)
}

fn validate_result(result: &ExampleAssessment, manifest: &Manifest) -> Result<()> {
    if result.protocol_version != 1
        || result.engine_build_id != manifest.engine_build_id
        || result.methodology_version != METHOD
        || result.mode != "fictional"
        || result.fixture_id != manifest.fixture_id
        || result.as_of != "2026-10-12T04:30:00.000Z"
        || result.currency != "USD"
        || result.input_hash.len() != 64
        || result.cards.len() != 3
        || result.horizons.iter().map(|v| v.years).collect::<Vec<_>>() != [1, 3, 5, 6, 7, 8]
        || result
            .cards
            .iter()
            .map(|v| v.years.clone())
            .collect::<Vec<_>>()
            != [vec![1], vec![3], vec![5, 6, 7, 8]]
    {
        return Err(failure("CALCULATOR_RESULT_INVALID"));
    }
    let states = [
        "input_required",
        "data_unavailable",
        "stale",
        "conflict",
        "unsupported",
        "constraints_failed",
        "scenarios_only",
        "model_unvalidated",
        "ready",
    ];
    if !result
        .input_hash
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit())
        || result
            .cards
            .iter()
            .any(|card| !states.contains(&card.state.as_str()))
        || result.reason_codes.len() > 256
        || result.reason_codes.iter().any(|code| code.len() > 120)
    {
        return Err(failure("CALCULATOR_RESULT_INVALID"));
    }
    for value in [
        &result.quantity,
        &result.invested,
        &result.entry_costs,
        &result.cash_remainder,
    ] {
        if value.len() > 64 || value.parse::<rust_decimal::Decimal>().is_err() {
            return Err(failure("CALCULATOR_RESULT_INVALID"));
        }
    }
    for value in &result.horizons {
        if chrono::NaiveDate::parse_from_str(&value.end, "%Y-%m-%d").is_err()
            || [
                value.remaining_a,
                value.remaining_b,
                value.difference,
                value.target,
            ]
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e18)
        {
            return Err(failure("CALCULATOR_RESULT_INVALID"));
        }
    }
    Ok(())
}

async fn run(
    directory: PathBuf,
    manifest: Manifest,
    case_id: ExampleCase,
    mut cancelled: oneshot::Receiver<()>,
) -> Result<ExampleAssessment> {
    let bytes=serde_json::to_vec(&serde_json::json!({"protocolVersion":1,"operation":"assess-company","mode":"fictional","fixtureId":manifest.fixture_id,"caseId":case_id,"engineBuildId":manifest.engine_build_id})).map_err(|_| failure("CALCULATOR_INPUT_INVALID"))?;
    if bytes.len() > LIMIT {
        return Err(failure("CALCULATOR_INPUT_LIMIT"));
    }
    let mut command = Command::new(directory.join(format!("node{}", std::env::consts::EXE_SUFFIX)));
    command
        .args([
            "--permission",
            &format!("--allow-fs-read={}", directory.join("engine.mjs").display()),
            "--no-addons",
            "--disable-proto=throw",
            "--disallow-code-generation-from-strings",
            "--max-old-space-size=64",
        ])
        .arg(directory.join("engine.mjs"))
        .current_dir(&directory)
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for key in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| failure("CALCULATOR_START_FAILED"))?;
    #[cfg(windows)]
    let _job = match super::windows_job::ChildJob::attach(&child) {
        Ok(job) => job,
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(failure("CALCULATOR_JOB_UNAVAILABLE"));
        }
    };
    #[cfg(all(test, windows))]
    if let Some(pid) = child.id() {
        TEST_CHILD_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
    }
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| failure("CALCULATOR_IO_FAILED"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| failure("CALCULATOR_IO_FAILED"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| failure("CALCULATOR_IO_FAILED"))?;
    let result = {
        let execution = async {
            let write = async move {
                stdin
                    .write_all(&bytes)
                    .await
                    .map_err(|_| failure("CALCULATOR_IO_FAILED"))?;
                stdin
                    .shutdown()
                    .await
                    .map_err(|_| failure("CALCULATOR_IO_FAILED"))?;
                // A pipe shutdown flushes but does not close its Windows handle.
                // Drop it explicitly so the calculator receives EOF before we wait.
                drop(stdin);
                Ok::<_, Error>(())
            };
            let wait = async {
                child
                    .wait()
                    .await
                    .map_err(|_| failure("CALCULATOR_IO_FAILED"))
            };
            let (_, output, _diagnostic, status) = tokio::try_join!(
                write,
                read_bounded(stdout, LIMIT),
                read_bounded(stderr, 4096),
                wait
            )?;
            if !status.success() {
                return Err(failure("CALCULATOR_REJECTED"));
            }
            let result: ExampleAssessment = serde_json::from_slice(&output)
                .map_err(|_| failure("CALCULATOR_RESULT_INVALID"))?;
            validate_result(&result, &manifest)?;
            if std::mem::discriminant(&result.case_id) != std::mem::discriminant(&case_id) {
                return Err(failure("CALCULATOR_RESULT_INVALID"));
            }
            Ok(result)
        };
        tokio::select! {
            value=execution=>value,
            _=tokio::time::sleep(std::time::Duration::from_secs(10))=>Err(failure("CALCULATOR_TIMEOUT")),
            _=&mut cancelled=>Err(failure("CALCULATOR_CANCELLED")),
        }
    };
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}

/// The host chooses a fixed package beside its own executable, never a request path.
pub async fn calculate_example(case_id: ExampleCase) -> Result<ExampleAssessment> {
    let executable = std::env::current_exe().map_err(|_| failure("CALCULATOR_UNAVAILABLE"))?;
    let directory = executable
        .parent()
        .ok_or_else(|| failure("CALCULATOR_UNAVAILABLE"))?
        .join("research");
    calculate_from_package(directory, case_id).await
}

async fn calculate_from_package(
    directory: PathBuf,
    case_id: ExampleCase,
) -> Result<ExampleAssessment> {
    // Node's Windows module loader rejects Rust's canonical extended drive
    // prefix. Use the same absolute local drive path for loading and permission.
    #[cfg(windows)]
    let directory = {
        let raw = directory
            .to_str()
            .ok_or_else(|| failure("CALCULATOR_PACKAGE_INVALID"))?;
        match raw.strip_prefix("\\\\?\\") {
            Some(local) if local.as_bytes().get(1) == Some(&b':') => PathBuf::from(local),
            Some(_) => return Err(failure("CALCULATOR_PACKAGE_INVALID")),
            None => directory,
        }
    };
    if !directory.is_absolute() {
        return Err(failure("CALCULATOR_PACKAGE_INVALID"));
    }
    let permit = SLOT.try_acquire().map_err(|_| failure("CALCULATOR_BUSY"))?;
    let (cancellation, mut cancelled) = oneshot::channel();
    // Owned task reaps the child even if the request future is dropped. Dropping the
    // sender signals cancellation; the slot belongs to the cleanup task until done.
    let task = tokio::spawn(async move {
        let _permit = permit;
        // Retain the single slot through verification too: cancelling a request
        // must not leave overlapping unbounded checksum jobs on the blocking pool.
        let check_directory = directory.clone();
        let manifest = tokio::task::spawn_blocking(move || verify_package(&check_directory))
            .await
            .map_err(|_| failure("CALCULATOR_PACKAGE_INVALID"))??;
        if !matches!(
            cancelled.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ) {
            return Err(failure("CALCULATOR_CANCELLED"));
        }
        run(directory, manifest, case_id, cancelled).await
    });
    let result = task.await.map_err(|_| failure("CALCULATOR_STOPPED"))?;
    drop(cancellation);
    result
}

#[cfg(all(test, windows))]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::panic,
        reason = "fictional process fixtures"
    )]
    use super::*;

    fn package() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../wealthfolio-trial-support/research-candidate/research")
            .canonicalize()
            .unwrap()
    }

    fn process_fixture(script: &str) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_path_buf();
        std::fs::copy(package().join("node.exe"), path.join("node.exe")).unwrap();
        std::fs::write(path.join("engine.mjs"), script).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(package().join("manifest.json")).unwrap())
                .unwrap();
        manifest["bundleSha256"] = hash_file(&path.join("engine.mjs"), LIMIT as u64)
            .unwrap()
            .into();
        std::fs::write(
            path.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        (directory, path)
    }

    #[tokio::test]
    #[ignore = "requires the prepared Windows research calculator package; run with --ignored"]
    async fn supervisor_checks_package_output_limits_cancellation_and_timeout() {
        if std::env::var_os("WC_RESEARCH_STOP_PROBE").is_some() {
            let mut command = Command::new(package().join("node.exe"));
            command
                .args(["-e", "setInterval(()=>{},1000);"])
                .env_clear()
                .kill_on_drop(true)
                .creation_flags(0x08000000);
            let child = command.spawn().unwrap();
            let _job = super::super::windows_job::ChildJob::attach(&child).unwrap();
            println!("OWNED_CALCULATOR_PID:{}", child.id().unwrap());
            use std::io::Write;
            std::io::stdout().flush().unwrap();
            std::future::pending::<()>().await;
        }
        // One sequential process test uses the same host-wide single slot as production.
        for (case, quantity) in [
            (ExampleCase::Baseline, "0.99000000"),
            (ExampleCase::ChangedFee, "0.95000000"),
            (ExampleCase::UnknownLoss, "0.99000000"),
        ] {
            let value = calculate_from_package(package(), case).await.unwrap();
            assert_eq!(value.quantity, quantity);
            assert_eq!(value.horizons.len(), 6);
            assert!(value.cards.iter().all(|card| card.score.is_none()));
            let mut invalid = value.clone();
            invalid.currency = "EUR".into();
            assert!(validate_result(&invalid, &verify_package(&package()).unwrap()).is_err());
            let mut extra = serde_json::to_value(value).unwrap();
            extra["privatePayload"] = true.into();
            assert!(serde_json::from_value::<ExampleAssessment>(extra).is_err());
        }
        assert!(serde_json::from_value::<ExampleRequest>(
            serde_json::json!({"caseId":"baseline","result":{}})
        )
        .is_err());
        for (script, code) in [
            ("process.stdout.write('{}');", "CALCULATOR_RESULT_INVALID"),
            (
                "process.stdout.write('x'.repeat(2097153));",
                "CALCULATOR_OUTPUT_LIMIT",
            ),
            (
                "process.stderr.write('x'.repeat(4097));",
                "CALCULATOR_OUTPUT_LIMIT",
            ),
            ("process.exit(1);", "CALCULATOR_REJECTED"),
        ] {
            let (_directory, path) = process_fixture(script);
            let error = calculate_from_package(path, ExampleCase::Baseline)
                .await
                .unwrap_err();
            assert!(error.to_string().contains(code), "{code}: {error}");
        }
        let (_directory, path) = process_fixture("setInterval(()=>{},1000);");
        TEST_CHILD_PID.store(0, std::sync::atomic::Ordering::SeqCst);
        let running = tokio::spawn(calculate_from_package(path.clone(), ExampleCase::Baseline));
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            while TEST_CHILD_PID.load(std::sync::atomic::Ordering::SeqCst) == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let cancelled_pid = TEST_CHILD_PID.load(std::sync::atomic::Ordering::SeqCst);
        assert!(calculate_from_package(path.clone(), ExampleCase::Baseline)
            .await
            .unwrap_err()
            .to_string()
            .contains("CALCULATOR_BUSY"));
        running.abort();
        let _ = running.await;
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while SLOT.available_permits() == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        // Signal zero checks existence; it never terminates another process.
        let mut probe = std::process::Command::new(package().join("node.exe"));
        probe.args(["-e", "try { process.kill(Number(process.argv[1]), 0); process.exit(1); } catch { process.exit(0); }", &cancelled_pid.to_string()]);
        use std::os::windows::process::CommandExt;
        probe.creation_flags(0x08000000);
        assert!(
            probe.status().unwrap().success(),
            "Cancelled child must be reaped"
        );
        let error = calculate_from_package(path.clone(), ExampleCase::Baseline)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("CALCULATOR_TIMEOUT"));
        std::fs::write(path.join("engine.mjs"), "changed package").unwrap();
        assert!(calculate_from_package(path, ExampleCase::Baseline)
            .await
            .unwrap_err()
            .to_string()
            .contains("CALCULATOR_PACKAGE_INVALID"));
        assert_eq!(SLOT.available_permits(), 1);

        // Force-stop a disposable host after it has assigned its live calculator
        // to a job. This does not depend on Rust destructors running in that host.
        let mut host_command = Command::new(std::env::current_exe().unwrap());
        host_command.args(["research::calculator::tests::supervisor_checks_package_output_limits_cancellation_and_timeout", "--ignored", "--nocapture"])
            .env("WC_RESEARCH_STOP_PROBE", "1").stdout(std::process::Stdio::piped()).kill_on_drop(true).creation_flags(0x08000000);
        let mut host = host_command.spawn().unwrap();
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(host.stdout.take().unwrap()).lines();
        let pid = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let line = lines
                    .next_line()
                    .await
                    .unwrap()
                    .expect("Probe host must report its owned child");
                if let Some(pid) = line.strip_prefix("OWNED_CALCULATOR_PID:") {
                    break pid.parse::<u32>().unwrap();
                }
            }
        })
        .await
        .unwrap();
        host.kill().await.unwrap();
        host.wait().await.unwrap();
        let mut probe = std::process::Command::new(package().join("node.exe"));
        probe.args(["-e", "try { process.kill(Number(process.argv[1]), 0); process.exit(1); } catch { process.exit(0); }", &pid.to_string()]).creation_flags(0x08000000);
        assert!(
            probe.status().unwrap().success(),
            "Forced host exit must terminate its calculator"
        );
    }
}
