pub mod agent {
    pub mod v1 {
        tonic::include_proto!("pandar.agent.v1");
    }
}

#[cfg(test)]
mod tests;
