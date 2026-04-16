use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Human,
    Json,
}

#[derive(Debug, Serialize)]
pub struct JsonOutput<T: Serialize> {
    pub ok: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonError>,
}

#[derive(Debug, Serialize)]
pub struct JsonError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl<T: Serialize> JsonOutput<T> {
    pub fn success(command: impl Into<String>, data: T) -> Self {
        Self {
            ok: true,
            command: command.into(),
            data: Some(data),
            error: None,
        }
    }
}

impl JsonOutput<()> {
    pub fn error(
        command: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            ok: false,
            command: command.into(),
            data: None,
            error: Some(JsonError {
                code: code.into(),
                message: message.into(),
                hint,
            }),
        }
    }
}

#[derive(Clone)]
pub struct Renderer {
    mode: OutputMode,
}

impl Renderer {
    pub fn new(json: bool) -> Self {
        Self {
            mode: if json {
                OutputMode::Json
            } else {
                OutputMode::Human
            },
        }
    }

    pub fn is_json(&self) -> bool {
        self.mode == OutputMode::Json
    }

    pub fn print_json<T: Serialize>(&self, output: &JsonOutput<T>) {
        debug_assert!(self.is_json(), "print_json called in human mode");
        match serde_json::to_string_pretty(output) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("{{\"ok\":false,\"error\":\"JSON serialization failed: {e}\"}}");
            }
        }
    }
}
