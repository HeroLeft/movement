use crate::Runner;
use dot_movement::DotMovement;
use m1_da_light_node_util::Config;

#[derive(Debug, Clone)]
pub struct Local;

impl Local {
	pub fn new() -> Self {
		Local
	}
}

impl Runner for Local {
	async fn run(&self, dot_movement: &DotMovement, config: Config) -> Result<(), anyhow::Error> {
		// celestia-appd start --grpc.enable --home $CELESTIA_APP_PATH --log_level $LOG_LEVEL
		commander::run_command(
			"celestia-appd",
			&[
				"start",
				"--grpc.enable",
				"--home",
				&config.try_celestia_app_path()?,
				"--log_level",
				"FATAL",
			],
		)
		.await?;

		Ok(())
	}
}
