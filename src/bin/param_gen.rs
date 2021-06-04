use manta_api::write_zkp_keys;
use manta_error::MantaError;

fn main() -> Result<(), MantaError> {
	println!("Hello, Manta!");
	write_zkp_keys()
}
