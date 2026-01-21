use std::sync::Arc;

use tokio::io::AsyncBufReadExt;

#[aviutl2::plugin(GenericPlugin)]
struct Vi5Aux2 {
    runtime: Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>,
    server: Option<(tokio::process::Child, vi5_cef::Client)>,
    project_dir: tokio::sync::Mutex<Option<String>>,
}

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
        let runtime_handle = self.get_runtime_handle();
        runtime_handle.block_on(self.set_project_dir(dir_str))?;
        Ok(())
    }
}
impl Vi5Aux2 {
    async fn set_project_dir(&mut self, dir: String) -> anyhow::Result<()> {
        aviutl2::log::info!("Setting project directory to: {}", dir);
        let metadata = tokio::fs::metadata(&dir).await.map_err(|e| {
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

        let mut guard = self.project_dir.lock().await;
        let client = match self.server.as_mut() {
            Some((_, client)) => {
                // 既存のサーバーがある場合は再利用
                client
            }
            None => {
                // 新しいサーバーを起動
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
                self.server = Some((child, client));
                let server = self.server.as_mut().unwrap();
                &mut server.1
            }
        };

        *guard = Some(dir);
        client
            .initialize(guard.as_deref().unwrap())
            .await
            .map_err(|e| anyhow::anyhow!("vi5.cef クライアントの初期化に失敗しました: {}", e))?;

        Ok(())
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
    fn block_on<F, R>(&self, fut: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        let guard = self.runtime.read().unwrap();
        let runtime = guard.as_ref().expect("Runtime has been shut down");
        runtime.block_on(fut)
    }
}

impl aviutl2::generic::GenericPlugin for Vi5Aux2 {
    fn new(_info: aviutl2::AviUtl2Info) -> aviutl2::AnyResult<Self> {
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
            server: None,
            project_dir: tokio::sync::Mutex::new(None),
        })
    }

    fn register(&mut self, host_app_handle: &mut aviutl2::generic::HostAppHandle) {
        host_app_handle.set_plugin_information(&format!(
            "vi5.aux2 / https://github.com/sevenc-nanashi/vi5.aux2 / v{}",
            env!("CARGO_PKG_VERSION")
        ));
        host_app_handle.register_menus::<Vi5Aux2>();
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
        if let Some((mut child, _client)) = self.server.take() {
            let _ = futures::executor::block_on(child.kill());
        }
        if let Some(runtime) = self.runtime.write().unwrap().take() {
            runtime.shutdown_timeout(std::time::Duration::from_secs(10));
        }
    }
}

aviutl2::register_generic_plugin!(Vi5Aux2);
