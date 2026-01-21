mod module;
use std::sync::{Arc, OnceLock};

use aviutl2::anyhow;
use tap::prelude::*;
use tokio::io::AsyncBufReadExt;

type Vi5Server = Arc<
    tokio::sync::Mutex<
        Option<(
            Arc<tokio::sync::Mutex<tokio::process::Child>>,
            vi5_cef::Client,
        )>,
    >,
>;

#[aviutl2::plugin(GenericPlugin)]
struct Vi5Aux2 {
    runtime: Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>,
    server: Vi5Server,
    project_dir: Arc<tokio::sync::Mutex<Option<String>>>,

    plugin: aviutl2::generic::SubPlugin<crate::module::InternalModule>,
}

fn get_script_dir(project_name: &str) -> std::path::PathBuf {
    process_path::get_dylib_path()
        .expect("Failed to get dylib path (unreachable on Windows)")
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("Script")
        .join(format!("vi5.aux2_{}", project_name))
}

static EDIT_HANDLE: OnceLock<aviutl2::generic::EditHandle> = OnceLock::new();

#[aviutl2::generic::menus]
impl Vi5Aux2 {
    #[config(name = "[vi5.aux2] プロジェクトフォルダの設定")]
    fn select_project_dir(&mut self, _hwnd: aviutl2::Win32WindowHandle) -> anyhow::Result<()> {
        let dir = rfd::FileDialog::new()
            .set_title("プロジェクトフォルダを選択してください")
            .pick_folder();
        let Some(dir) = dir else {
            return Ok(());
        };
        let dir_str = dir.to_string_lossy().to_string();
        self.set_project_dir(dir_str)?;
        Ok(())
    }
}
impl Vi5Aux2 {
    fn set_project_dir(&mut self, dir: String) -> anyhow::Result<()> {
        aviutl2::log::info!("Setting project directory to: {}", dir);
        let metadata = std::fs::metadata(&dir).map_err(|e| {
            anyhow::anyhow!(
                "指定されたフォルダのメタデータを取得できませんでした: {}: {}",
                dir,
                e
            )
        })?;
        if !metadata.is_dir() {
            return Err(anyhow::anyhow!(
                "指定されたパスはフォルダではありません: {}",
                dir
            ));
        }

        *self.project_dir.blocking_lock() = Some(dir.clone());

        let runtime_handle = self.get_runtime_handle();
        let project_dir = self.project_dir.clone();
        let server = self.server.clone();
        runtime_handle.spawn(async move {
            if let Err(e) = Self::initialize_project_dir(dir, project_dir, server).await {
                aviutl2::log::error!("Failed to initialize project directory: {}", e);
            }
        });
        Ok(())
    }

    async fn initialize_project_dir(
        dir: String,
        project_dir: Arc<tokio::sync::Mutex<Option<String>>>,
        server: Vi5Server,
    ) -> anyhow::Result<()> {
        {
            let guard = project_dir.lock().await;
            if guard.as_deref() != Some(dir.as_str()) {
                return Ok(());
            }
        }

        let mut server_guard = server.lock().await;
        if server_guard.is_none() {
            let (child, client) = Self::start_vi5_cef_server().await.inspect_err(|e| {
                let _ = native_dialog::DialogBuilder::message()
                    .set_title("vi5.aux2")
                    .set_text(format!("vi5-cef サーバーの起動に失敗しました:\n{}", e))
                    .set_level(native_dialog::MessageLevel::Error)
                    .alert()
                    .show();
            })?;
            let child = Arc::new(tokio::sync::Mutex::new(child));
            Self::spawn_vi5_cef_exit_logger(child.clone());
            *server_guard = Some((child, client));
        }
        let client = server_guard.as_mut().map(|(_, client)| client).unwrap();

        let info = client
            .initialize(&dir)
            .await
            .map_err(|e| anyhow::anyhow!("vi5-cef クライアントの初期化に失敗しました: {}", e))?;
        aviutl2::log::info!("vi5-cef initialized successfully.");
        let mut requires_restart = false;
        let mut requires_reload = false;

        let module_name = process_path::get_dylib_path()
            .expect("Failed to get dylib path (unreachable on Windows)")
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let script_dir = get_script_dir(&info.project_name);
        aviutl2::log::info!("Project script directory: {:?}", script_dir);
        if !script_dir.exists() {
            tokio::fs::create_dir_all(&script_dir).await?;
            requires_restart = true;
        }

        for object in info.object_infos {
            let base_script = include_str!("./script.lua").to_string();
            let param_defs = object
                .parameter_definitions
                .iter()
                .enumerate()
                .map(|(i, param)| {
                    let label = &param.label;
                    match param.parameter_type {
                        vi5_cef::ParameterType::String => {
                            format!(r#"--value{i}:{label},"""#)
                        }
                        vi5_cef::ParameterType::Text => {
                            format!(r#"--text{i}:{label},"#)
                        }
                        vi5_cef::ParameterType::Boolean => {
                            format!(r#"--check{i}:{label},"#)
                        }
                        vi5_cef::ParameterType::Number { step, min, max } => {
                            let min_str = min.to_string();
                            let max_str = max.to_string();
                            let step = step.as_str();
                            format!(r#"--track{i}:{label},{min_str},{max_str},{step}"#)
                        }
                        vi5_cef::ParameterType::Color => {
                            format!(r#"--color{i}:{label},"#)
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let keys = object
                .parameter_definitions
                .iter()
                .map(|param| serde_json::to_string(&param.key).unwrap())
                .collect::<Vec<_>>()
                .join(",");
            let values = object
                .parameter_definitions
                .iter()
                .enumerate()
                .map(|(i, param)| match param.parameter_type {
                    vi5_cef::ParameterType::String => {
                        format!(r#"value{i}"#)
                    }
                    vi5_cef::ParameterType::Text => {
                        format!(r#"text{i}"#)
                    }
                    vi5_cef::ParameterType::Boolean => {
                        format!(r#"check{i}"#)
                    }
                    vi5_cef::ParameterType::Number { .. } => {
                        format!(r#"track{i} or obj.track{i} or 0"#)
                    }
                    vi5_cef::ParameterType::Color => {
                        format!(r#"color{i}"#)
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            let types = object
                .parameter_definitions
                .iter()
                .map(|param| match param.parameter_type {
                    vi5_cef::ParameterType::String => r#""Str""#.to_string(),
                    vi5_cef::ParameterType::Text => r#""Text""#.to_string(),
                    vi5_cef::ParameterType::Boolean => r#""Bool""#.to_string(),
                    vi5_cef::ParameterType::Number { .. } => r#""Number""#.to_string(),
                    vi5_cef::ParameterType::Color => r#""Color""#.to_string(),
                })
                .collect::<Vec<_>>()
                .join(",");

            let script_content = base_script
                .replace("--PARAMETER_DEFINITIONS--", param_defs.as_str())
                .replace(
                    "--LABEL--",
                    format!("--label:vi5.aux2\\{}", info.project_name).as_str(),
                )
                .replace("--MODULE_NAME--", module_name.as_str())
                .replace("--PARAMETER_KEYS--", keys.as_str())
                .replace("--PARAMETER_VALUES--", values.as_str())
                .replace(r#"--PARAMETER_TYPES--"#, types.as_str())
                .replace(
                    r#""--OBJECT_ID--""#,
                    serde_json::to_string(&object.id).unwrap().as_str(),
                );
            let script_path = script_dir.join(format!("{}.obj2", object.label));
            aviutl2::log::info!(
                "Loaded script for object '{}': {:?}",
                object.id,
                script_path
            );
            if script_path.exists() {
                let existing_content = tokio::fs::read_to_string(&script_path).await?;
                let existing_headers = existing_content
                    .lines()
                    .take_while(|line| line == &"--END_HEADER")
                    .collect::<Vec<_>>()
                    .join("\n");
                let new_headers = script_content
                    .lines()
                    .take_while(|line| line == &"--END_HEADER")
                    .collect::<Vec<_>>()
                    .join("\n");
                if existing_content != script_content {
                    tokio::fs::write(&script_path, script_content).await?;
                    aviutl2::log::info!(
                        "Updated script file for object '{}': {:?}",
                        object.id,
                        script_path
                    );
                    if existing_headers != new_headers {
                        aviutl2::log::warn!(
                            "Script file for object '{}' has updated headers",
                            object.id,
                        );
                        requires_restart = true;
                    } else {
                        requires_reload = true;
                    }
                }
            } else {
                tokio::fs::write(&script_path, script_content).await?;
                aviutl2::log::info!(
                    "Created script file for object '{}': {:?}",
                    object.id,
                    script_path
                );
                requires_restart = true;
            }
        }

        if requires_restart {
            aviutl2::log::info!("Script directory updated.");
            let will_restart = native_dialog::DialogBuilder::message()
                .set_title("vi5.aux2")
                .set_text("オブジェクトが更新されました。\nAviUtl2を再起動しますか？")
                .confirm()
                .spawn()
                .await?;
            if will_restart {
                aviutl2::log::info!("Restarting AviUtl2...");
                if let Some(edit_handle) = EDIT_HANDLE.get() {
                    edit_handle.restart_host_app();
                }
            }
        } else if requires_reload {
            aviutl2::log::info!("Script directory updated.");
            native_dialog::DialogBuilder::message()
                .set_title("vi5.aux2")
                .set_text(
                    "オブジェクトが更新されました。\nF5を押してスクリプトモジュールをリロードしてください。",
                )
                .alert()
                .spawn()
                .await?;
        }

        Ok(())
    }

    async fn start_vi5_cef_server() -> anyhow::Result<(tokio::process::Child, vi5_cef::Client)> {
        // TODO: port を動的に決定する
        let port = 50051;
        let mut path = std::env::var("PATH").unwrap_or_default();
        path.push_str(";C:\\Users\\seven\\.local\\share\\cef");
        aviutl2::log::info!("Starting vi5-cef server on port {}", port);
        for p in path.split(';') {
            aviutl2::log::debug!("PATH entry: {}", p);
        }
        // TODO: 実行ファイルのパスを適切に設定する
        let cef_server_path =
            std::path::Path::new("e:/aviutl2/vi5.aux2/target/release/vi5-cef-server.exe");
        let mut child = tokio::process::Command::new(cef_server_path)
            .arg("--port")
            .arg(port.to_string())
            .arg("--hardware-acceleration")
            .arg("--devtools")
            .env("PATH", path)
            .env("NO_COLOR", "1")
            // NOTE: C:\Windows\System32 で起動するとなぜかlibcef.dllを見つけられなくて落ちるので、カレントディレクトリを実行ファイルのディレクトリにする
            .current_dir(cef_server_path.parent().unwrap())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .tap(|cmd| {
                aviutl2::log::info!("Launching vi5-cef server: {cmd:?}");
            })
            .spawn()
            .map_err(|e| anyhow::anyhow!("vi5-cef サーバーの起動に失敗しました: {}", e))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        tokio::spawn(async move {
            let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
            while let Some(line) = stdout_reader.next_line().await.transpose() {
                if let Ok(line) = line {
                    aviutl2::log::debug!("[vi5-cef-server stdout] {}", line);
                }
            }
        });
        tokio::spawn(async move {
            let mut stderr_reader = tokio::io::BufReader::new(stderr).lines();
            while let Some(line) = stderr_reader.next_line().await.transpose() {
                if let Ok(line) = line {
                    aviutl2::log::error!("[vi5-cef-server stderr] {}", line);
                }
            }
        });
        let client = tokio::select! {
            code = child.wait() => {
                anyhow::bail!("初期化に失敗しました (exit code: {:?})", code);
            }

            res = vi5_cef::Client::connect_with_timeout(
                format!("http://localhost:{}", port),
                std::time::Duration::from_secs(60),
            ) => {
                res.map_err(anyhow::Error::from)
            }
        }?;
        Ok((child, client))
    }

    fn spawn_vi5_cef_exit_logger(child: Arc<tokio::sync::Mutex<tokio::process::Child>>) {
        tokio::spawn(async move {
            loop {
                let status = {
                    let mut child = child.lock().await;
                    match child.try_wait() {
                        Ok(Some(status)) => Some(status),
                        Ok(None) => None,
                        Err(e) => {
                            aviutl2::log::error!(
                                "Failed to wait for vi5-cef server process: {}",
                                e
                            );
                            return;
                        }
                    }
                };
                if let Some(status) = status {
                    match status.code() {
                        Some(0) => {
                            aviutl2::log::info!("vi5-cef server has exited normally.");
                        }
                        Some(code) => {
                            aviutl2::log::error!("vi5-cef server has exited (exit code: {})", code);
                        }
                        None => {
                            aviutl2::log::error!("vi5-cef server has exited (unknown exit code)");
                        }
                    }
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        });
    }

    fn get_runtime_handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            runtime: self.runtime.clone(),
        }
    }

    pub async fn with_client<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: AsyncFnOnce(&mut vi5_cef::Client) -> anyhow::Result<R>,
    {
        let mut server_guard = self.server.lock().await;
        let Some((_, client)) = server_guard.as_mut() else {
            anyhow::bail!("vi5-cef server is not running")
        };
        f(client).await
    }
}

struct RuntimeHandle {
    runtime: Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>,
}
impl RuntimeHandle {
    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let guard = self.runtime.read().unwrap();
        let runtime = guard.as_ref().expect("Runtime has been shut down");
        runtime.spawn(fut);
    }
}

impl aviutl2::generic::GenericPlugin for Vi5Aux2 {
    fn new(info: aviutl2::AviUtl2Info) -> aviutl2::AnyResult<Self> {
        aviutl2::logger::LogBuilder::new()
            .filter_level(if cfg!(debug_assertions) {
                aviutl2::logger::LevelFilter::Debug
            } else {
                aviutl2::logger::LevelFilter::Info
            })
            .init();
        Ok(Self {
            runtime: Arc::new(std::sync::RwLock::new(Some(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .unwrap(),
            ))),
            server: Arc::new(tokio::sync::Mutex::new(None)),
            project_dir: Arc::new(tokio::sync::Mutex::new(None)),
            plugin: aviutl2::generic::SubPlugin::new_script_module(info)?,
        })
    }

    fn register(&mut self, host_app_handle: &mut aviutl2::generic::HostAppHandle) {
        host_app_handle.set_plugin_information(&format!(
            "vi5.aux2 / https://github.com/sevenc-nanashi/vi5.aux2 / v{}",
            env!("CARGO_PKG_VERSION")
        ));
        host_app_handle.register_menus::<Vi5Aux2>();
        host_app_handle.register_script_module(&self.plugin);
        EDIT_HANDLE.get_or_init(|| host_app_handle.create_edit_handle());
    }

    fn on_project_load(&mut self, project: &mut aviutl2::generic::ProjectFile) {
        let mut guard = self.project_dir.blocking_lock();
        *guard = match project.deserialize::<String>("project_dir") {
            Ok(dir) => {
                if dir.is_empty() {
                    None
                } else {
                    Some(dir)
                }
            }
            Err(e) => {
                aviutl2::log::error!("Failed to get project parameter: {}", e);
                None
            }
        }
    }

    fn on_project_save(&mut self, project: &mut aviutl2::generic::ProjectFile) {
        project.clear_params();
        if let Err(e) = project.serialize(
            "project_dir",
            &self.project_dir.blocking_lock().as_deref().unwrap_or(""),
        ) {
            aviutl2::log::error!("Failed to set project parameter: {}", e);
        }
    }
}

impl Drop for Vi5Aux2 {
    fn drop(&mut self) {
        if let Some((child, _client)) = self.server.blocking_lock().take() {
            aviutl2::log::info!("Shutting down vi5-cef server...");
            futures::executor::block_on(async {
                let mut child = child.lock().await;
                let _ = child.kill().await;
            });
        }
        if let Some(runtime) = self.runtime.write().unwrap().take() {
            aviutl2::log::info!("Shutting down Tokio runtime...");
            runtime.shutdown_timeout(std::time::Duration::from_secs(10));
        }
    }
}

aviutl2::register_generic_plugin!(Vi5Aux2);
