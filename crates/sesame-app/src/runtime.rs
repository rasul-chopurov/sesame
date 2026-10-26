//! One Tokio runtime for SSH and other network work. GPUI has its own
//! executor; UI code spawns onto this runtime and awaits the JoinHandle.

use std::sync::OnceLock;

use tokio::runtime::Runtime;

pub fn get() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("sesame-io")
            .build()
            .expect("tokio runtime")
    })
}
