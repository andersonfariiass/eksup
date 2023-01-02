mod aws;
mod cli;
mod k8s;
mod playbook;

use std::process;

use anyhow::*;
use aws_config::meta::region::RegionProviderChain;
use clap::Parser;
pub use cli::{Cli, Commands};
// pub use k8s::{Discovery, Deprecated};

pub const LATEST: &str = "1.24";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
  let cli = Cli::parse();

  match &cli.command {
    Commands::CreatePlaybook(args) => {
      let cluster_version = args.cluster_version.to_string();
      if LATEST.eq(&cluster_version) {
        println!("Cluster is already at the latest supported version: {cluster_version}");
        println!("Nothing to upgrade at this time");
        return Ok(());
      }

      if let Err(err) = playbook::create(args) {
        eprintln!("{err}");
        process::exit(2);
      }
    }

    Commands::Analyze(args) => {
      // Query Kubernetes first so that we can get AWS details that require further querying
      // Or make it where Kubernetes module can query AWS?

      let client = kube::Client::try_default().await?;

      let deprecated = k8s::Deprecated::get()?;
      let discovery = k8s::Discovery::get(&client).await?;

      // Checks if any of the deprecated APIs are still supported by the API server
      for (key, value) in &deprecated.versions {
        if discovery.versions.contains_key(key) {
          println!("DEPRECATED: {value:#?}");
        }
      }

      let region_provider =
        RegionProviderChain::first_try(args.region.clone().map(aws_sdk_eks::Region::new))
          .or_default_provider();

      let aws_shared_config = aws_config::from_env().region(region_provider).load().await;

      let eks_client = aws_sdk_eks::Client::new(&aws_shared_config);
      let cluster = aws::get_cluster(&eks_client, &args.cluster_name).await?;
      println!("{cluster:#?}");

      let ec2_client = aws_sdk_ec2::Client::new(&aws_shared_config);
      let subnet_ids = cluster
        .resources_vpc_config()
        .unwrap()
        .subnet_ids
        .as_ref()
        .unwrap();
      let subnets = aws::get_subnets(&ec2_client, subnet_ids.clone()).await?;
      println!("{subnets:#?}");
    }
  }

  Ok(())
}
