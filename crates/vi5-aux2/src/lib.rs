mod module;
use std::sync::{Arc, OnceLock};

use tokio::io::AsyncBufReadExt;

#[aviutl2::plugin(GenericPlugin)]
struct Vi5Aux2 {
    runtime: Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>,
    server: Arc<tokio::sync::Mutex<Option<(tokio::process::Child, vi5_cef::Client)>>>,
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
        server: Arc<tokio::sync::Mutex<Option<(tokio::process::Child, vi5_cef::Client)>>>,
    ) -> anyhow::Result<()> {
        {
            let guard = project_dir.lock().await;
            if guard.as_deref() != Some(dir.as_str()) {
                return Ok(());
            }
        }

        let mut server_guard = server.lock().await;
        if server_guard.is_none() {
            *server_guard = Some(Self::start_vi5_cef_server().await?);
        }
        let client = server_guard.as_mut().map(|(_, client)| client).unwrap();

        let info = client
            .initialize(&dir)
            .await
            .map_err(|e| anyhow::anyhow!("vi5-cef クライアントの初期化に失敗しました: {}", e))?;
        aviutl2::log::info!("vi5-cef initialized successfully.");
        let mut updated = false;
        let script_dir = get_script_dir(&info.project_name);
        aviutl2::log::info!("Project script directory: {:?}", script_dir);
        if !script_dir.exists() {
            tokio::fs::create_dir_all(&script_dir).await?;
            updated = true;
        }

        for object in info.object_infos {
            aviutl2::log::info!("Loaded object: {object:?}");

            let base_script = include_str!("./script.lua").to_string();
            let param_defs = object
                .parameter_definitions
                .iter()
                .map(|param| {
                    let key = &param.key;
                    let label = &param.label;
                    match param.parameter_type {
                        vi5_cef::ParameterType::String => {
                            format!(r#"--value@{key}:{label},"""#)
                        }
                        vi5_cef::ParameterType::Text => {
                            format!(r#"--text@{key}:{label},"#)
                        }
                        vi5_cef::ParameterType::Boolean => {
                            format!(r#"--check@{key}:{label},"#)
                        }
                        vi5_cef::ParameterType::Number { step, min, max } => {
                            let min_str = min.map_or("-1000".to_string(), |v| v.to_string());
                            let max_str = max.map_or("1000".to_string(), |v| v.to_string());
                            let step = "0.01"; // TODO: step を反映する
                            format!(r#"--track@{key}:{label},{min_str},{max_str},{step}"#)
                        }
                        vi5_cef::ParameterType::Color => {
                            format!(r#"--color@{key}:{label},"#)
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            let script_content =
                base_script.replace("--PARAMETER_DEFINITIONS--", param_defs.as_str());
            let script_path = script_dir.join(format!("{}.obj2", object.id));
            if !script_path.exists() {
                tokio::fs::write(&script_path, script_content).await?;
                aviutl2::log::info!(
                    "Created script file for object '{}': {:?}",
                    object.id,
                    script_path
                );
                updated = true;
            }
        }

        if updated {
            aviutl2::log::info!("Script directory updated.");
            let will_restart = native_dialog::DialogBuilder::message()
                .set_title("vi5.aux2")
                .set_text(
                    "新しいオブジェクトがプロジェクトフォルダに追加されました。\nAviUtl2を再起動しますか？",
                )
                .confirm()
                .spawn()
                .await?;
            if will_restart {
                aviutl2::log::info!("Restarting AviUtl2...");
                if let Some(edit_handle) = EDIT_HANDLE.get() {
                    edit_handle.restart_host_app();
                }
            }
        }

        Ok(())
    }

    async fn start_vi5_cef_server() -> anyhow::Result<(tokio::process::Child, vi5_cef::Client)> {
        // TODO: port を動的に決定する
        let port = 50051;
        let mut path = std::env::var("PATH").unwrap_or_default();
        path.push_str(";c:/users/seven/.local/share/cef");
        aviutl2::log::info!("Starting vi5-cef server on port {}", port);
        let mut child = tokio::process::Command::new(
            // TODO: 実行ファイルのパスを適切に設定する
            "e:/aviutl2/vi5.aux2/target/debug/vi5-cef-server.exe",
        )
        .arg("--port")
        .arg(port.to_string())
        .arg("--hardware-acceleration")
        .arg("--devtools")
        .env("PATH", path)
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| anyhow::anyhow!("vi5-cef サーバーの起動に失敗しました: {}", e))?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        tokio::spawn(async move {
            let mut stdout_reader = tokio::io::BufReader::new(stdout).lines();
            while let Some(line) = stdout_reader.next_line().await.transpose() {
                if let Ok(line) = line {
                    aviutl2::log::info!("[vi5-cef-server stdout] {}", line);
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
        let client = vi5_cef::Client::connect_with_timeout(
            format!("http://localhost:{}", port),
            std::time::Duration::from_secs(30),
        )
        .await
        .map_err(|e| anyhow::anyhow!("vi5-cef サーバーへの接続に失敗しました: {}", e))?;
        Ok((child, client))
    }

    fn get_runtime_handle(&self) -> RuntimeHandle {
        RuntimeHandle {
            runtime: self.runtime.clone(),
        }
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
        *guard = match project.get_param_string("project_dir") {
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
        if let Err(e) = project.set_param_string(
            "project_dir",
            self.project_dir.blocking_lock().as_deref().unwrap_or(""),
        ) {
            aviutl2::log::error!("Failed to set project parameter: {}", e);
        }
    }
}

impl Drop for Vi5Aux2 {
    fn drop(&mut self) {
        if let Some((mut child, _client)) = self.server.blocking_lock().take() {
            let _ = futures::executor::block_on(child.kill());
        }
        if let Some(runtime) = self.runtime.write().unwrap().take() {
            runtime.shutdown_timeout(std::time::Duration::from_secs(10));
        }
    }
}

aviutl2::register_generic_plugin!(Vi5Aux2);
