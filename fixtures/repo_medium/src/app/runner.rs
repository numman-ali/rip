use super::config::Config;

pub fn run(cfg: Config) -> String {
    format!("ok: {} (retries={})", cfg.name, cfg.retries)
}

