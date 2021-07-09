#!/usr/bin/env bash

########################## Check Manta-Node ##########################

git clone https://github.com/Manta-Network/Manta.git

cd Manta/

cargo update -p manta-error
cargo update -p manta-crypto
cargo update -p manta-asset
cargo update -p manta-ledger
cargo update -p manta-data
cargo update -p manta-api

sed -i "s@pallet-manta-pay = { git='https://github.com/Manta-Network/pallet-manta-pay', branch='calamari', default-features = false }@pallet-manta-pay = {path= '../../../../', default-features = false }@g" ./runtimes/manta/runtime/Cargo.toml
         
cargo build
cargo build --all-features
