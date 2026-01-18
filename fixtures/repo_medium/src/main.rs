mod app;

fn main() {
    let cfg = app::config::Config::default();
    let status = app::runner::run(cfg);
    println!("{status}");
}

