//! Device regression probe, launched by CommandBackendsDeviceTest with app-private paths.
use clawseed_api::tool::Tool;
use clawseed_api::tool_context::ToolContext;
use clawseed_tools::background::{
    BackgroundCancelTool, BackgroundJobManager, BackgroundRunTool, BackgroundStatusTool,
};
use clawseed_tools::linux_exec::LinuxExecTool;
use clawseed_tools::shell::ShellTool;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct Context(PathBuf);
impl ToolContext for Context {
    fn workspace_dir(&self) -> &Path {
        &self.0
    }
}

async fn invoke(tool: &dyn Tool, args: Value, context: &Context) -> anyhow::Result<String> {
    let result = tool.execute(args, context).await?;
    anyhow::ensure!(result.success, "{}: {:?}", tool.name(), result.error);
    Ok(result.output)
}

async fn terminal(
    status: &BackgroundStatusTool,
    job: &str,
    context: &Context,
) -> anyhow::Result<Value> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let state: Value =
                serde_json::from_str(&invoke(status, json!({"job_id":job}), context).await?)?;
            if matches!(
                state["status"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                return Ok(state);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}

fn main() -> anyhow::Result<()> {
    eprintln!("probe: native entry");
    #[cfg(unix)]
    unsafe {
        let mut mask: libc::sigset_t = std::mem::zeroed();
        libc::sigprocmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask);
        eprintln!(
            "probe: inherited SIGQUIT blocked={}",
            libc::sigismember(&mask, libc::SIGQUIT)
        );
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}

async fn run() -> anyhow::Result<()> {
    eprintln!("probe: start");
    let context = Context(
        std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("workspace argument is required"))?,
    );
    std::fs::create_dir_all(context.workspace_dir())?;
    let shell = ShellTool::new();
    let output = invoke(
        &shell,
        json!({"command":"printf host","timeout_secs":5}),
        &context,
    )
    .await?;
    anyhow::ensure!(output == "host", "host backend output: {output}");
    eprintln!("probe: host passed");
    let output = invoke(&shell, json!({"backend":"linux", "timeout_secs":5, "command":
        "python3 -c 'print(\"python-ok\")'; node -p '\"node-ok\"'; printf shared > /workspace/shared.txt"}), &context).await?;
    anyhow::ensure!(
        output.contains("python-ok") && output.contains("node-ok"),
        "Linux output: {output}"
    );
    anyhow::ensure!(
        std::fs::read_to_string(context.0.join("shared.txt"))? == "shared",
        "workspace binding"
    );
    eprintln!("probe: linux workspace passed");
    anyhow::ensure!(
        invoke(
            &LinuxExecTool::new(),
            json!({"command":"printf alias","timeout_secs":5}),
            &context
        )
        .await?
            == "alias",
        "legacy alias"
    );

    eprintln!("probe: alias passed");
    let (sender, mut events) = tokio::sync::mpsc::unbounded_channel();
    let manager = BackgroundJobManager::persistent(
        context.0.join("job-store"),
        Some(std::sync::Arc::new(move |event| {
            let _ = sender.send(event);
        })),
    );
    let run = BackgroundRunTool::new(manager.clone());
    let status = BackgroundStatusTool::new(manager.clone());
    let cancel = BackgroundCancelTool::new(manager);
    for backend in ["host", "linux"] {
        eprintln!("probe: background {backend}");
        let start: Value = serde_json::from_str(
            &invoke(
                &run,
                json!({"backend":backend,"command":"printf background-ok"}),
                &context,
            )
            .await?,
        )?;
        let job = start["job_id"].as_str().unwrap();
        let state = terminal(&status, job, &context).await?;
        anyhow::ensure!(
            state["status"] == "succeeded"
                && state["stdout"] == "background-ok"
                && state["backend"] == backend,
            "{state}"
        );
        let event = tokio::time::timeout(Duration::from_secs(3), events.recv())
            .await?
            .unwrap();
        anyhow::ensure!(event.job_id == job, "completion event");
    }
    let start: Value = serde_json::from_str(&invoke(&run, json!({"backend":"linux", "timeout_secs":1,"command":"printf timeout-output; sleep 10"}), &context).await?)?;
    let state = terminal(&status, start["job_id"].as_str().unwrap(), &context).await?;
    anyhow::ensure!(
        state["status"] == "failed"
            && state["error"] == "timed_out"
            && state["stdout"] == "timeout-output",
        "{state}"
    );

    eprintln!("probe: background and timeout passed");
    let mut running = Vec::new();
    for number in 0..2 {
        let start: Value = serde_json::from_str(
            &invoke(
                &run,
                json!({"backend":"linux", "command":
            format!("printf started > /workspace/running-{number}; sleep 30")}),
                &context,
            )
            .await?,
        )?;
        running.push(start["job_id"].as_str().unwrap().to_string());
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while !(context.0.join("running-0").exists() && context.0.join("running-1").exists()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    let queued: Value = serde_json::from_str(
        &invoke(
            &run,
            json!({"backend":"linux", "command":"touch /workspace/queued-ran"}),
            &context,
        )
        .await?,
    )?;
    let result: Value = serde_json::from_str(
        &invoke(&cancel, json!({"job_id":queued["job_id"]}), &context).await?,
    )?;
    anyhow::ensure!(
        result["status"] == "cancelled",
        "queued cancellation: {result}"
    );
    eprintln!("probe: queue cancellation passed");
    for job in running {
        let result: Value =
            serde_json::from_str(&invoke(&cancel, json!({"job_id":job}), &context).await?)?;
        anyhow::ensure!(
            result["status"] == "cancelled",
            "running cancellation: {result}"
        );
    }
    anyhow::ensure!(
        !context.0.join("queued-ran").exists(),
        "queued job unexpectedly executed"
    );
    eprintln!("probe: running cancellation passed");
    // Cancellation must kill tracees, including a child that would write after a delay.
    let detached_child = r#"node -e 'const fs=require("fs"),cp=require("child_process"); const child=cp.spawn("/bin/sh",["-c","sleep 3; touch /workspace/orphan"],{detached:true,stdio:"inherit"}); fs.writeFileSync("/workspace/child-started","started"); child.on("exit",code=>process.exit(code||0));'"#;
    let start: Value = serde_json::from_str(
        &invoke(
            &run,
            json!({"backend":"linux", "command":detached_child}),
            &context,
        )
        .await?,
    )?;
    tokio::time::timeout(Duration::from_secs(5), async {
        while !context.0.join("child-started").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    anyhow::ensure!(
        !context.0.join("orphan").exists(),
        "child finished before cancellation could be tested"
    );
    invoke(&cancel, json!({"job_id":start["job_id"]}), &context).await?;
    tokio::time::sleep(Duration::from_millis(3200)).await;
    anyhow::ensure!(
        !context.0.join("orphan").exists(),
        "cancelled child survived"
    );
    let summary: Value = serde_json::from_str(&invoke(&status, json!({}), &context).await?)?;
    anyhow::ensure!(
        summary["active_count"] == 0 && summary["storage_available"] == true,
        "persistent job summary: {summary}"
    );
    anyhow::ensure!(
        context
            .0
            .join("job-store")
            .read_dir()?
            .flatten()
            .any(|entry| entry.path().extension().is_some_and(|ext| ext == "json")),
        "no persisted job snapshots"
    );
    println!(
        "COMMAND_BACKENDS_OK host linux python node alias background events timeout cancel workspace"
    );
    Ok(())
}
