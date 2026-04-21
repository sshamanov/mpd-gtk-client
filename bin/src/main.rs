use anyhow::Result;
use log::info;
use state::create_initial_state;
use ui::App;

fn main() -> Result<()> {
    env_logger::init();
    info!("Starting MPD client");

    let state = create_initial_state();
    let app = App::new(state);
    app.run();

    info!("Exiting");
    Ok(())
}