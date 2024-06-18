pub mod local;

pub trait M1DaLightNodeSetupOperations {

    async fn setup(
        &self,
        dot_movement : dot_movement::DotMovement,
        config : m1_da_light_node_util::Config,
    ) -> Result<m1_da_light_node_util::Config, anyhow::Error>;

}