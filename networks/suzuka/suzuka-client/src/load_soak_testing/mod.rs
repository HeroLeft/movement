use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::{fs::File, sync::Arc};
use tracing_subscriber::{filter, prelude::*};

mod scenario;
pub use scenario::Scenario;

const EXEC_LOG_FILTER: &str = "exec";

<<<<<<< HEAD
/// Initialize all test components with the configuration.
/// Must be called before the tests start: execute_test
=======
/// Initialize all test's components with the configuration.
/// Must be call before the test start: execute_test
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
pub fn init_test(config: &ExecutionConfig) -> Result<(), std::io::Error> {
	//do some verification on the config
	config.verify_config();

	// init tracing to log error in a file and stdout
	// and log execution data in a json file.
	let stdout_log = tracing_subscriber::fmt::layer().pretty();

	// A layer that logs error and warn event to a file.
	let log_file = File::create(&config.logfile)?;
	let file_log = tracing_subscriber::fmt::layer().with_writer(Arc::new(log_file));

	// A layer that logs execution event to a file.
	let exec_file = File::create(&config.execfile)?;
	let execution_log = tracing_subscriber::fmt::layer().json().with_writer(Arc::new(exec_file));

	tracing_subscriber::registry()
		.with(
			stdout_log
				.with_filter(filter::LevelFilter::INFO)
				.and_then(file_log.with_filter(filter::LevelFilter::WARN))
				// Add a filter that rejects spans and
				// events whose targets start with `exec`.
				.with_filter(filter::filter_fn(|metadata| {
					!metadata.target().starts_with(EXEC_LOG_FILTER)
				})),
		)
		.with(
			// Add a filter to the exec label that *only* enables
			// events whose targets start with `exec`.
			execution_log.with_filter(filter::filter_fn(|metadata| {
				metadata.target().starts_with(EXEC_LOG_FILTER)
			})),
		)
		.init();
	tracing::info!("Load and Soak test inited with config:{config:?}");
	Ok(())
}

<<<<<<< HEAD
/// Defines how the test will be run:
#[derive(Clone, Debug)]
pub struct ExecutionConfig {
	/// Type of test to run
	pub kind: TestKind,
	/// The path to the file where log WARN and ERROR are written
	pub logfile: String,
	/// The path to the file where execution data are written to be processed later.
	pub execfile: String,
	/// The number of started scenarios per client. number_scenarios / number_scenario_per_client defines the number of clients.
	pub number_scenario_per_client: usize,
=======
/// Define how the test will be run:
/// * kind: Type of test to run
/// * logfile_path: the file where log WARN and ERROR are written
/// * execfile_path: File where execution data are written to be processed later.
/// * define the number of started scenario per client. nb_scenarios / nb_scenario_per_client define the number of client.
#[derive(Clone, Debug)]
pub struct ExecutionConfig {
	pub kind: TestKind,
	pub logfile: String,
	pub execfile: String,
	pub nb_scenario_per_client: usize,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
}

impl ExecutionConfig {
	fn verify_config(&self) {
		match self.kind {
<<<<<<< HEAD
			TestKind::Load { number_scenarios } => {
				assert!(
					number_scenarios >= self.number_scenario_per_client,
=======
			TestKind::Load { nb_scenarios } => {
				assert!(
					nb_scenarios >= self.nb_scenario_per_client,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
					"Number of running scenario less than the number if scenario per client."
				);
			}
			TestKind::Soak { min_scenarios, max_scenarios, .. } => {
				assert!(max_scenarios >= min_scenarios, "max scenarios less than min scenarios");
				assert!(
<<<<<<< HEAD
					min_scenarios >= self.number_scenario_per_client,
=======
					min_scenarios >= self.nb_scenario_per_client,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
					"Number of min running scenario less than the number if scenario per client."
				);
			}
		}
	}
}

impl Default for ExecutionConfig {
	fn default() -> Self {
<<<<<<< HEAD
		let number_scenarios: usize = std::env::var("LOADTEST_NUMBER_SCENARIO")
			.map_err(|err| err.to_string())
			.and_then(|val| val.parse().map_err(|err: std::num::ParseIntError| err.to_string()))
			.unwrap_or(10);
		let number_scenario_per_client: usize =
			std::env::var("LOADTEST_NUMBER_SCENARIO_PER_CLIENT")
				.unwrap_or("2".to_string())
				.parse()
				.unwrap_or(2);
		ExecutionConfig {
			kind: TestKind::build_load_test(number_scenarios),
			logfile: "log_file.txt".to_string(),
			execfile: "test_result.txt".to_string(),
			number_scenario_per_client,
=======
		let nb_scenarios: usize = std::env::var("LOADTEST_NB_SCENARIO")
			.unwrap_or("10".to_string())
			.parse()
			.unwrap_or(10);
		let nb_scenario_per_client: usize = std::env::var("LOADTEST_NB_SCENARIO_PER_CLIENT")
			.unwrap_or("2".to_string())
			.parse()
			.unwrap_or(2);
		ExecutionConfig {
			kind: TestKind::build_load_test(nb_scenarios),
			logfile: "log_file.txt".to_string(),
			execfile: "test_result.txt".to_string(),
			nb_scenario_per_client,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
		}
	}
}

/// Define the type of test to run:
<<<<<<< HEAD
#[derive(Clone, Debug)]
pub enum TestKind {
	/// Load: try to run all scenario (number_scenarios) concurrently
	Load { number_scenarios: usize },
	/// Soak: start min_scenarios at first then increase the number to max_scenarios then decrease and do number_cycle during duration
=======
/// * Load: try to run all scenario (nb_scenarios) concurrently
/// * Soak: start min_scenarios at first then increase the number to max_scenarios then decrease and do nb_clycle during duration
#[derive(Clone, Debug)]
pub enum TestKind {
	Load {
		nb_scenarios: usize,
	},
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
	Soak {
		min_scenarios: usize,
		max_scenarios: usize,
		duration: std::time::Duration,
<<<<<<< HEAD
		number_cycle: u32,
=======
		nb_clycle: u32,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
	},
}

impl TestKind {
<<<<<<< HEAD
	pub fn build_load_test(number_scenarios: usize) -> Self {
		TestKind::Load { number_scenarios }
=======
	pub fn build_load_test(nb_scenarios: usize) -> Self {
		TestKind::Load { nb_scenarios }
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
	}
	pub fn build_soak_test(
		min_scenarios: usize,
		max_scenarios: usize,
		duration: std::time::Duration,
<<<<<<< HEAD
		number_cycle: u32,
	) -> Self {
		TestKind::Soak { min_scenarios, max_scenarios, duration, number_cycle }
	}
}

/// Execute the test scenarios defined in the specified configuration.
/// scenarios are executed by chunk. Each chunk of execution is done by a client.
/// All clients are executed in a different thread in parallel.
/// Clients execute scenarios in a Tokio runtime concurrently.
pub fn execute_test(config: ExecutionConfig, create_scenario: Arc<scenario::CreateScenarioFn>) {
	tracing::info!("Start test scenario execution.");

	let number_scenarios = match config.kind {
		TestKind::Load { number_scenarios } => number_scenarios,
=======
		nb_clycle: u32,
	) -> Self {
		TestKind::Soak { min_scenarios, max_scenarios, duration, nb_clycle }
	}
}

/// Execute the test scenarios define in the specified configuration.
/// scenarios are executed by chunk. Chunk execution of scenario is done by a client.
/// All clients are executed in a different thread in parallel.
/// Clients execute scenario in a Tokio runtime concurrently.
pub fn execute_test(config: ExecutionConfig, create_scenario: Arc<scenario::CreateScenarioFn>) {
	tracing::info!("Start test scenario execution.");

	let nb_scenarios = match config.kind {
		TestKind::Load { nb_scenarios } => nb_scenarios,
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
		TestKind::Soak { max_scenarios, .. } => max_scenarios,
	};

	//build chunk of ids. Start at 1. 0 mean in result execution fail before scenario can execute.
<<<<<<< HEAD
	let ids: Vec<_> = (1..=number_scenarios).collect();
	let chunks: Vec<_> = ids
		.into_iter()
		.chunks(config.number_scenario_per_client)
=======
	let ids: Vec<_> = (1..=nb_scenarios).collect();
	let chunks: Vec<_> = ids
		.into_iter()
		.chunks(config.nb_scenario_per_client)
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
		.into_iter()
		.map(|chunk| {
			(config.kind.clone(), chunk.into_iter().collect::<Vec<_>>(), create_scenario.clone())
		})
		.collect();
	// Execute the client by id's chunk.
	let exec_results: Vec<_> = chunks
		.into_par_iter()
		.map(|(kind, chunk, create_scenario)| {
			let client = TestClient::new(chunk);
			client.run_scenarios(kind.clone(), create_scenario.clone())
		})
		.collect();

	let no_zero_exec_time: Vec<_> = exec_results
		.into_iter()
		.filter_map(|res| (res.average_execution_time_milli > 0).then_some(res))
		.collect();

	let average_exec_time = no_zero_exec_time
		.iter()
		.map(|res| res.average_execution_time_milli)
		.sum::<u128>()
		/ no_zero_exec_time.len() as u128;
	let metrics_average_exec_time = serde_json::to_string(&average_exec_time)
		.unwrap_or("Metric  execution result serialization error.".to_string());
	tracing::info!(target:EXEC_LOG_FILTER, metrics_average_exec_time);
	tracing::info!("Scenarios execution average_exec_time:{metrics_average_exec_time}");

	tracing::info!("End test scenario execution.");
}

<<<<<<< HEAD
/// Runs the specified scenarios concurrently using Tokio.
=======
/// Run the specified scenarios concurrently using Tokio.
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
#[derive(Default)]
struct TestClient {
	scenario_chunk: Vec<usize>,
}

impl TestClient {
	fn new(scenario_chunk: Vec<usize>) -> Self {
		TestClient { scenario_chunk }
	}

	fn run_scenarios(
		self,
		kind: TestKind,
		create_scanario: Arc<scenario::CreateScenarioFn>,
	) -> ClientExecResult {
		// Start the Tokio runtime on the current thread
<<<<<<< HEAD
		let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
			Ok(rt) => rt,
			Err(err) => panic!("Tokio RT runtime fail to start because of this error:{err}"),
		};
		let scenario_results = match kind {
			TestKind::Load { .. } => rt.block_on(self.load_runner(create_scanario.clone())),
			TestKind::Soak { min_scenarios, max_scenarios, duration, number_cycle } => {
=======
		let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
		let scenario_results = match kind {
			TestKind::Load { .. } => rt.block_on(self.load_runner(create_scanario.clone())),
			TestKind::Soak { min_scenarios, max_scenarios, duration, nb_clycle } => {
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				// The scenario that run all the time and part time are divided using the client.
				// min_scenarios first ids are run permanently, the others client run part time.
				//ids start at 1.
				if *self.scenario_chunk.last().unwrap_or(&min_scenarios) <= min_scenarios {
					// Start scenarios that run all the time.
					rt.block_on(self.soak_runner_in_a_loop(create_scanario.clone(), duration))
				} else {
					//TODO

					// In soak test, scenario are rerun until the end of the test.
					// min_scenarios run all the time.
					// The others scenarios start after some time (start delta time) then run the same time: Part-time scenario duration
					// max_scenarios - min_scenarios scenarios run part-time depending on the number of cycle.
<<<<<<< HEAD
					// Part-time scenario duration max: Duration / (number_cycle * 2)
					// scenario start delta: (Part-time scenario duration max * scenario index / nb scenario) + (Duration * current cycle / nb cycle)
					let _number_part_time_scenario: u32 = (max_scenarios - min_scenarios) as u32;
					let _parttime_scenario_duration = duration / (number_cycle * 2);
=======
					// Part-time scenario duration max: Duration / (nbcycle * 2)
					// scenario start delta: (Part-time scenario duration max * scenario index / nb scenario) + (Duration * current cycle / nb cycle)
					let nb_parttime_scenario: u32 = (max_scenarios - min_scenarios) as u32;
					let parttime_scenario_duration = duration / (nb_clycle * 2);
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
					vec![]
				}
			}
		};

		let exec_results = ClientExecResult::new(scenario_results);
		let metrics_client_execution = serde_json::to_string(&exec_results)
			.unwrap_or("Metric client result serialization error.".to_string());
		tracing::info!(target:EXEC_LOG_FILTER, metrics_client_execution);
		exec_results
	}

	async fn load_runner(
		self,
		create_scanario: Arc<scenario::CreateScenarioFn>,
	) -> Vec<ScenarioExecMetric> {
		//start all client's scenario
		let mut set = tokio::task::JoinSet::new();
		let start_time = std::time::Instant::now();
		self.scenario_chunk.into_iter().for_each(|id| {
			let scenario = create_scanario(id);
			set.spawn(futures::future::join(futures::future::ready(id), scenario.run()));
		});
		let mut scenario_results = vec![];
		while let Some(res) = set.join_next().await {
			let elapse = start_time.elapsed().as_millis();
			let metrics = match res {
<<<<<<< HEAD
				Ok((id, Ok(()))) => ScenarioExecMetric::new(id, elapse, ScenarioExecResult::Ok),
=======
				Ok((id, Ok(()))) => ScenarioExecMetric::new_ok(id, elapse),
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				Ok((id, Err(err))) => {
					let log = format!("Scenario:{id} execution failed because: {err}");
					tracing::info!(target:EXEC_LOG_FILTER, log);
					tracing::warn!(log);
<<<<<<< HEAD
					ScenarioExecMetric::new(id, elapse, ScenarioExecResult::Fail)
				}
				Err(err) => {
					tracing::warn!("Error during scenario spawning: {err}");
					ScenarioExecMetric::new(0, elapse, ScenarioExecResult::Fail)
=======
					ScenarioExecMetric::new_err(id, elapse)
				}
				Err(err) => {
					tracing::warn!("Error during scenario spawning: {err}");
					ScenarioExecMetric::new_err(0, elapse)
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				}
			};
			let metrics_scenario = serde_json::to_string(&metrics)
				.unwrap_or("Metric serialization error.".to_string());
			tracing::info!(target:EXEC_LOG_FILTER, metrics_scenario);
			scenario_results.push(metrics);
		}
		scenario_results
	}

	async fn soak_runner_in_a_loop(
		self,
		create_scanario: Arc<scenario::CreateScenarioFn>,
		duration: std::time::Duration,
	) -> Vec<ScenarioExecMetric> {
		let initial_start_time = std::time::Instant::now();

		let mut set = tokio::task::JoinSet::new();
		//start min scenario
		self.scenario_chunk.into_iter().for_each(|id| {
			let create_scanario = create_scanario.clone();
			set.spawn(futures::future::join(
				futures::future::ready(id),
				run_scenarion_in_loop(id, create_scanario, duration.clone()),
			));
		});

		let mut scenario_results = vec![];
		while let Some(res) = set.join_next().await {
			let metrics = match res {
<<<<<<< HEAD
				Ok((id, Ok(elapse))) => ScenarioExecMetric::new(id, elapse, ScenarioExecResult::Ok),
=======
				Ok((id, Ok(elapse))) => ScenarioExecMetric::new_ok(id, elapse),
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				Ok((id, Err(err))) => {
					let log = format!("Scenario:{id} execution failed because: {err}");
					tracing::info!(target:EXEC_LOG_FILTER, log);
					tracing::warn!(log);
					let elapse = initial_start_time.elapsed().as_millis();
<<<<<<< HEAD
					ScenarioExecMetric::new(id, elapse, ScenarioExecResult::Fail)
=======
					ScenarioExecMetric::new_err(id, elapse)
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				}
				Err(err) => {
					tracing::warn!("Error during scenario spawning: {err}");
					let elapse = initial_start_time.elapsed().as_millis();
<<<<<<< HEAD
					ScenarioExecMetric::new(0, elapse, ScenarioExecResult::Fail)
=======
					ScenarioExecMetric::new_err(0, elapse)
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
				}
			};
			let metrics_scenario = serde_json::to_string(&metrics)
				.unwrap_or("Metric serialization error.".to_string());
			tracing::info!(target:EXEC_LOG_FILTER, metrics_scenario);
			scenario_results.push(metrics);
		}
		scenario_results
	}
}

async fn run_scenarion_in_loop(
	id: usize,
	create_scanario: Arc<scenario::CreateScenarioFn>,
	duration: Duration,
<<<<<<< HEAD
) -> Result<u128, anyhow::Error> {
=======
) -> anyhow::Result<u128> {
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
	let start_time = std::time::Instant::now();
	let mut average_time = 0;
	loop {
		let elapse = start_time.elapsed();
		if elapse > duration {
			break;
		}

		tracing::info!("{id} start new test");
		let exec_start_time = std::time::Instant::now();
		let scenario = create_scanario(id);
		scenario.run().await?;
		let exec_elapse = exec_start_time.elapsed().as_millis();
		if average_time == 0 {
			average_time = exec_elapse;
		} else {
			average_time = (exec_elapse + average_time) / 2;
		}
		tracing::info!("{id} end test exec_elapse:{exec_elapse} average_time:{average_time}");
	}
	Ok(average_time)
}

#[derive(Serialize, Deserialize)]
struct ScenarioExecMetric {
	scenario_id: usize,
	elapse_millli: u128,
	result: ScenarioExecResult,
}

impl ScenarioExecMetric {
<<<<<<< HEAD
	fn new(scenario_id: usize, elapse_millli: u128, result: ScenarioExecResult) -> Self {
		ScenarioExecMetric { scenario_id, elapse_millli, result }
=======
	fn new_ok(scenario_id: usize, elapse_millli: u128) -> Self {
		ScenarioExecMetric { scenario_id, elapse_millli, result: ScenarioExecResult::Ok }
	}
	fn new_err(scenario_id: usize, elapse_millli: u128) -> Self {
		ScenarioExecMetric { scenario_id, elapse_millli, result: ScenarioExecResult::Fail }
>>>>>>> 186a4994 (recreate the PR to remove unknown modifications)
	}

	fn is_ok(&self) -> bool {
		match self.result {
			ScenarioExecResult::Ok => true,
			ScenarioExecResult::Fail => false,
		}
	}
}

#[derive(Serialize, Deserialize)]
enum ScenarioExecResult {
	Ok,
	Fail,
}

#[derive(Serialize, Deserialize, Debug)]
struct ClientExecResult {
	average_execution_time_milli: u128,
}

impl ClientExecResult {
	fn new(scenarios: Vec<ScenarioExecMetric>) -> Self {
		ClientExecResult {
			average_execution_time_milli: Self::calculate_average_exec_time_milli(&scenarios),
		}
	}

	pub fn calculate_average_exec_time_milli(scenarios: &[ScenarioExecMetric]) -> u128 {
		if !scenarios.is_empty() {
			let ok_scenario: Vec<_> = scenarios
				.into_iter()
				.filter_map(|s| if s.is_ok() { Some(s.elapse_millli) } else { None })
				.collect();
			ok_scenario.iter().sum::<u128>() / ok_scenario.len() as u128
		} else {
			tracing::warn!("No result available average exec time is 0");
			0
		}
	}
}
