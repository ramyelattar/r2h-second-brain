from pathlib import Path

ROOT = Path(r"E:\Projects\r2h-second-brain")
APP = ROOT / "apps" / "desktop" / "src" / "App.tsx"

text = APP.read_text(encoding="utf-8")
backup = APP.with_name("App.tsx.before-c4-3-listener-readiness")
backup.write_text(text, encoding="utf-8", newline="\n")


def replace_once(old: str, new: str, label: str) -> None:
    global text

    count = text.count(old)

    if count != 1:
        raise RuntimeError(
            f"{label}: expected exactly one match, found {count}"
        )

    text = text.replace(old, new, 1)


# ============================================================
# 1) Listener readiness state
# ============================================================

replace_once(
    '  const [streamStatus, setStreamStatus] = useState("");\n'
    '  const [loadingHistory, setLoadingHistory] = useState(false);',
    '  const [streamStatus, setStreamStatus] = useState("");\n'
    '  const [streamListenerReady, setStreamListenerReady] = useState(false);\n'
    '  const [loadingHistory, setLoadingHistory] = useState(false);',
    "listener readiness state",
)


# ============================================================
# 2) Combined chat readiness
# ============================================================

replace_once(
    '  const sending = activeRequestId !== null;\n',
    '  const sending = activeRequestId !== null;\n'
    '  const chatReady = runtimeReady && streamListenerReady;\n',
    "combined chat readiness",
)


# ============================================================
# 3) Harden listener initialization
# ============================================================

old_listener_start = '''  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<ChatStreamEventDto>("r2h-chat-stream", (event) => {
'''

new_listener_start = '''  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    setStreamListenerReady(false);

    void listen<ChatStreamEventDto>("r2h-chat-stream", (event) => {
'''

replace_once(
    old_listener_start,
    new_listener_start,
    "listener initialization",
)


old_listener_end = '''    }).then((dispose) => {
      if (disposed) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
'''

new_listener_end = '''    })
      .then((dispose) => {
        if (disposed) {
          dispose();
          return;
        }

        unlisten = dispose;
        setStreamListenerReady(true);
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setStreamListenerReady(false);
          setError(
            `Unable to initialize local chat events: ${errorMessage(reason)}`,
          );
        }
      });

    return () => {
      disposed = true;
      setStreamListenerReady(false);
      unlisten?.();
    };
  }, []);
'''

replace_once(
    old_listener_end,
    new_listener_end,
    "listener completion",
)


# ============================================================
# 4) Block submit until listener is ready
# ============================================================

replace_once(
    '    if (!value || !runtimeReady || sending) return;',
    '    if (!value || !chatReady || sending) return;',
    "submit readiness guard",
)


# ============================================================
# 5) Composer readiness
# ============================================================

replace_once(
    '''            placeholder={
              runtimeReady
                ? "Ask your second brain"
                : "Waiting for local AI runtime…"
            }
            disabled={!runtimeReady || sending}
''',
    '''            placeholder={
              !runtimeReady
                ? "Waiting for local AI runtime…"
                : !streamListenerReady
                  ? "Preparing secure local chat…"
                  : "Ask your second brain"
            }
            disabled={!chatReady || sending}
''',
    "composer readiness",
)

replace_once(
    '              disabled={!draft.trim() || !runtimeReady}',
    '              disabled={!draft.trim() || !chatReady}',
    "send button readiness",
)


# ============================================================
# 6) Visible status
# ============================================================

replace_once(
    '''            <StatusPill tone={runtimeReady ? "success" : "warning"}>
              {runtimeReady ? "R2H AI ready" : "Runtime unavailable"}
            </StatusPill>
''',
    '''            <StatusPill tone={chatReady ? "success" : "warning"}>
              {!runtimeReady
                ? "Runtime unavailable"
                : streamListenerReady
                  ? "R2H AI ready"
                  : "Preparing chat…"}
            </StatusPill>
''',
    "visible readiness status",
)


# ============================================================
# Verification
# ============================================================

required = [
    "streamListenerReady",
    "const chatReady = runtimeReady && streamListenerReady;",
    "setStreamListenerReady(true);",
    "setStreamListenerReady(false);",
    "if (!value || !chatReady || sending) return;",
    'disabled={!chatReady || sending}',
    'disabled={!draft.trim() || !chatReady}',
    "Preparing secure local chat…",
]

missing = [item for item in required if item not in text]

if missing:
    raise RuntimeError(
        "Listener readiness patch verification failed: "
        + ", ".join(missing)
    )

APP.write_text(text, encoding="utf-8", newline="\n")

print("C4_3_LISTENER_READINESS_FIXED")
print(f"BACKUP={backup}")
