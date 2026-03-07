use anyhow::Result;

pub fn fire(title: &str, body: &str) -> Result<()> {
    fire_impl(title, body)
}

#[cfg(target_os = "linux")]
fn fire_impl(title: &str, body: &str) -> Result<()> {
    std::process::Command::new("notify-send")
        .args([
            "--app-name",
            "ward",
            "--urgency",
            "normal",
            "--expire-time",
            "10000",
            title,
            body,
        ])
        .status()
        .ok();
    Ok(())
}

#[cfg(target_os = "macos")]
fn fire_impl(title: &str, body: &str) -> Result<()> {
    let safe_body = body.replace('"', "\\\"").replace('\n', " ");
    let safe_title = title.replace('"', "\\\"");
    let script = format!(
        "display notification \"{}\" with title \"ward\" subtitle \"{}\"",
        safe_body, safe_title,
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .status()
        .ok();
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn fire_impl(_title: &str, _body: &str) -> Result<()> {
    Ok(())
}
