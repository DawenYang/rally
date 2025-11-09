use std::{collections::HashMap, io};

use rusoto_ec2::Ec2;

struct SshConnection;

struct Machine {
    ssh: SshConnection,
    instance_type: String,
    ip: String,
    dns: String,
}

struct MachineSetup<F> {
    instance_type: String,
    ami: String,
    setup: F,
}

impl<F> MachineSetup<F>
where
    F: Fn(&mut SshConnection) -> io::Result<()>,
{
    pub fn new(instance_type: String, ami: String, setup: F) -> Self {
        MachineSetup {
            instance_type,
            ami,
            setup,
        }
    }
}

struct RallyBuilder {
    descriptors: HashMap<String, (MachineSetup, u32)>,
    max_duration: i64,
}

impl Default for RallyBuilder {
    fn default() -> Self {
        RallyBuilder {
            descriptors: Vec::new(),
            max_duration: i64::default(),
        }
    }
}

impl RallyBuilder {
    pub fn add_set(&mut self, name: String, number: u32, setup: MachineSetup) {
        self.descriptors.insert(name, (setup, number));
    }

    pub async fn run<F>(self)
    where
        F: FnOnce(HashMap<String, &mut [Machine]>) -> io::Result<()>,
    {
        let ec2 = rusoto_ec2::Ec2Client::new(rusoto_core::Region::ApSoutheast1);

        let mut spot_req_ids = Vec::new();
        for (name, (setup, number)) in self.descriptors {
            let mut launch = rusoto_ec2::RequestSpotLaunchSpecification::default();
            launch.image_id = Some(setup.ami);
            launch.instance_type = Some(setup.instance_type);

            let mut req = rusoto_ec2::RequestSpotInstancesRequest::default();
            req.instance_count = Some(i64::from(number));
            req.block_duration_minutes = Some(self.max_duration);

            let res = ec2.request_spot_instances(req).await.unwrap();
            let res = res.spot_instance_requests.unwrap();

            spot_req_ids.extend(
                res.into_iter()
                    .filter_map(|sir| sir.spot_instance_request_id),
            );
        }
    }
}
