from pathlib import Path

ROOT = Path(r"E:\Projects\r2h-second-brain")
CHAT = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "chat_bridge.rs"

text = CHAT.read_text(encoding="utf-8")

backup = CHAT.with_name("chat_bridge.rs.before-c4-3-stream-timeout")
backup.write_text(text, encoding="utf-8", newline="\n")

old_client = '''fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .no_proxy()
        .build()
        .map_err(|error| {
            format!("Unable to initialize local chat client: {error}")
        })
}
'''

new_client = '''fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .no_proxy()
        .build()
        .map_err(|error| {
            format!("Unable to initialize local chat client: {error}")
        })
}

fn streaming_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .no_proxy()
        .build()
        .map_err(|error| {
            format!(
                "Unable to initialize local streaming chat client: {error}"
            )
        })
}
'''

if old_client not in text:
    if "fn streaming_client()" in text:
        print("STREAMING_CLIENT_ALREADY_PRESENT")
    else:
        raise RuntimeError("Unable to locate current client() function")
else:
    text = text.replace(old_client, new_client, 1)

stream_start = text.find("pub async fn stream_chat(")
stream_end = text.find("\npub async fn send_chat(", stream_start)

if stream_start < 0 or stream_end < 0:
    raise RuntimeError("Unable to locate stream_chat function boundaries")

stream_block = text[stream_start:stream_end]

old_usage = "    let client = client()?;\n"
new_usage = "    let client = streaming_client()?;\n"

count = stream_block.count(old_usage)

if count == 1:
    stream_block = stream_block.replace(old_usage, new_usage, 1)
elif count == 0 and new_usage in stream_block:
    print("STREAM_CHAT_ALREADY_USES_STREAMING_CLIENT")
else:
    raise RuntimeError(
        f"Unexpected client initialization count inside stream_chat: {count}"
    )

text = text[:stream_start] + stream_block + text[stream_end:]

required = [
    "fn streaming_client()",
    "let client = streaming_client()?;",
    ".connect_timeout(Duration::from_secs(10))",
]

missing = [item for item in required if item not in text]

if missing:
    raise RuntimeError(
        "Stream timeout patch verification failed: " + ", ".join(missing)
    )

updated_stream_block = text[
    text.find("pub async fn stream_chat("):
    text.find("\npub async fn send_chat(")
]

if ".timeout(Duration::from_secs(300))" in updated_stream_block:
    raise RuntimeError(
        "The streaming path still contains the five-minute total timeout"
    )

CHAT.write_text(text, encoding="utf-8", newline="\n")

print("C4_3_STREAM_TOTAL_TIMEOUT_REMOVED")
print(f"BACKUP={backup}")
