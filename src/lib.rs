// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! # Manta pay Module
//!
//! A simple, secure module for manta anonymous payment
//!
//! ## Overview
//!
//! The Assets module provides functionality for asset management of fungible asset classes
//! with a fixed supply, including:
//!
//! * Asset Issuance
//! * Asset Transfer
//!
//!
//! To use it in your runtime, you need to implement the assets [`Trait`](./trait.Trait.html).
//!
//! The supported dispatchable functions are documented in the [`Call`](./enum.Call.html) enum.
//!
//! ### Terminology
//!
//! * **Asset issuance:** The creation of the asset (note: this asset can only be created once)
//! * **Asset transfer:** The action of transferring assets from one account to another.
//! * **Asset destruction:** The process of an account removing its entire holding of an asset.
//!
//! The assets system in Substrate is designed to make the following possible:
//!
//! * Issue a unique asset to its creator's account.
//! * Move assets between accounts.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! * `issue` - Issues the total supply of a new fungible asset to the account of the caller of the function.
//! * `transfer` - Transfers an `amount` of units of fungible asset `id` from the balance of
//! the function caller's account (`origin`) to a `target` account.
//! * `destroy` - Destroys the entire holding of a fungible asset `id` associated with the account
//! that called the function.
//!
//! Please refer to the [`Call`](./enum.Call.html) enum and its associated variants for documentation on each function.
//!
//! ### Public Functions
//! <!-- Original author of descriptions: @gavofyork -->
//!
//! * `balance` - Get the asset balance of `who`.
//! * `total_supply` - Get the total supply of an asset `id`.
//!
//! Please refer to the [`Module`](./struct.Module.html) struct for details on publicly available functions.
//!
//! ## Usage
//!
//! The following example shows how to use the Assets module in your runtime by exposing public functions to:
//!
//! * Initiate the fungible asset for a token distribution event (airdrop).
//! * Query the fungible asset holding balance of an account.
//! * Query the total supply of a fungible asset that has been issued.
//!
//! ### Prerequisites
//!
//! Import the Assets module and types and derive your runtime's configuration traits from the Assets module trait.
//!
//! ## Related Modules
//!
//! * [`System`](../frame_system/index.html)
//! * [`Support`](../frame_support/index.html)

// Ensure we're `no_std` when compiling for Wasm.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate ark_crypto_primitives;
extern crate ark_ed_on_bls12_381;
extern crate ark_groth16;
extern crate ark_r1cs_std;
extern crate ark_relations;
extern crate ark_serialize;
extern crate ark_std;
extern crate blake2;
extern crate generic_array;
extern crate rand_chacha;
extern crate x25519_dalek;

mod benchmark;
mod coin;
mod constants;
mod crypto;
mod param;
mod serdes;
mod shard;

#[cfg(test)]
mod test;

pub use coin::*;
pub use constants::{COMMIT_PARAM_BYTES, HASH_PARAM_BYTES, RECLAIM_VKBYTES, TRANSFER_VKBYTES};
pub use param::*;
pub use serdes::MantaSerDes;

// TODO: this interface is only exposed for benchmarking
// use a feature gate to control this expose
#[allow(unused_imports)]
pub use crypto::*;

use ark_std::vec::Vec;
use frame_support::{decl_error, decl_event, decl_module, decl_storage, ensure};
use frame_system::ensure_signed;
use serdes::Checksum;
use shard::*;
use sp_runtime::traits::{StaticLookup, Zero};

/// The module configuration trait.
pub trait Config: frame_system::Config {
	/// The overarching event type.
	type Event: From<Event<Self>> + Into<<Self as frame_system::Config>::Event>;
}

decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		type Error = Error<T>;

		fn deposit_event() = default;
		/// Issue a new class of fungible assets. There are, and will only ever be, `total`
		/// such assets and they'll all belong to the `origin` initially. It will have an
		/// identifier `AssetId` instance: this will be specified in the `Issued` event.
		/// __TODO__: check the weights is correct
		/// # <weight>
		/// - `O(1)`
		/// - 1 storage mutation (codec `O(1)`).
		/// - 2 storage writes (codec `O(1)`).
		/// - 1 event.
		/// # </weight>
		#[weight = 0]
		fn init(origin, total: u64) {

			ensure!(!Self::is_init(), <Error<T>>::AlreadyInitialized);
			let origin = ensure_signed(origin)?;

			// for now we hard code the parameters generated from the following seed:
			//  * hash parameter seed: [1u8; 32]
			//  * commitment parameter seed: [2u8; 32]
			// We may want to pass those two in for `init`
			let hash_param = HashParam::deserialize(HASH_PARAM_BYTES.as_ref());
			let commit_param = CommitmentParam::deserialize(COMMIT_PARAM_BYTES.as_ref());
			let hash_param_checksum = hash_param.get_checksum();
			let commit_param_checksum = commit_param.get_checksum();

			// push the ZKP verification key to the ledger storage
			//
			// NOTE:
			//    this is is generated via
			//      let zkp_key = priv_coin::manta_XXX_zkp_key_gen(&hash_param_seed, &commit_param_seed);
			//
			// for prototype, we use this function to generate the ZKP verification key
			// for product we should use a MPC protocol to build the ZKP verification key
			// and then deploy that vk
			//
			TransferZKPKey::put(TRANSFER_VKBYTES.to_vec());
			ReclaimZKPKey::put(RECLAIM_VKBYTES.to_vec());

			// coin_shards are a 256 lists of commitments
			let coin_shards = Shards::default();
			CoinShards::put(coin_shards);

			PoolBalance::put(0);
			VNList::put(Vec::<[u8; 32]>::new());
			EncValueList::put(Vec::<[u8; 16]>::new());
			<Balances<T>>::insert(&origin, total);
			<TotalSupply>::put(total);
			Self::deposit_event(RawEvent::Issued(origin, total));
			Init::put(true);
			HashParamChecksum::put(hash_param_checksum);
			CommitParamChecksum::put(commit_param_checksum);
		}

		/// Move some assets from one holder to another.
		/// __TODO__: check the weights is correct
		///
		/// # <weight>
		/// - `O(1)`
		/// - 1 static lookup
		/// - 2 storage mutations (codec `O(1)`).
		/// - 1 event.
		/// # </weight>
		#[weight = 0]
		fn transfer(origin,
			target: <T::Lookup as StaticLookup>::Source,
			amount: u64
		) {
			ensure!(Self::is_init(), <Error<T>>::BasecoinNotInit);
			let origin = ensure_signed(origin)?;

			let origin_account = origin.clone();
			let origin_balance = <Balances<T>>::get(&origin_account);
			let target = T::Lookup::lookup(target)?;
			ensure!(!amount.is_zero(), Error::<T>::AmountZero);
			ensure!(origin_balance >= amount, Error::<T>::BalanceLow);
			Self::deposit_event(RawEvent::Transferred(origin, target.clone(), amount));
			<Balances<T>>::insert(origin_account, origin_balance - amount);
			<Balances<T>>::mutate(target, |balance| *balance += amount);
		}

		/// Given an amount, and relevant data, mint the token to the ledger
		#[weight = 0]
		fn mint(origin,
			amount: u64,
			input_data: [u8; 96]
		) {
			// todo: Implement the fix denomination method

			// parse the input_data into input
			let input = MintData::deserialize(input_data.as_ref());

			// get the original balance
			ensure!(Self::is_init(), <Error<T>>::BasecoinNotInit);
			let origin = ensure_signed(origin)?;
			let origin_account = origin.clone();
			ensure!(!amount.is_zero(), Error::<T>::AmountZero);
			let origin_balance = <Balances<T>>::get(&origin_account);
			ensure!(origin_balance >= amount, Error::<T>::BalanceLow);

			let hash_param = HashParam::deserialize(HASH_PARAM_BYTES.as_ref());
			let commit_param = CommitmentParam::deserialize(COMMIT_PARAM_BYTES.as_ref());
			let hash_param_checksum_local = hash_param.get_checksum();
			let commit_param_checksum_local = commit_param.get_checksum();


			// get the parameter checksum from the ledger
			let hash_param_checksum = HashParamChecksum::get();
			let commit_param_checksum = CommitParamChecksum::get();
			ensure!(
				hash_param_checksum_local == hash_param_checksum,
				<Error<T>>::MintFail
			);
			ensure!(
				commit_param_checksum_local == commit_param_checksum,
				<Error<T>>::MintFail
			);
			// todo: checksum ZKP verification eky



			// check the validity of the commitment
			ensure!(
				input.sanity_check(amount, &commit_param),
				<Error<T>>::MintFail
			);

			// check cm is not in the ledger
			let mut coin_shards = CoinShards::get();
			ensure!(
				!coin_shards.exist(&input.cm),
				Error::<T>::MantaCoinExist
			);

			// update the shards
			coin_shards.update(&input.cm, hash_param);

			// write back to ledger storage
			Self::deposit_event(RawEvent::Minted(origin, amount));
			CoinShards::put(coin_shards);

			let old_pool_balance = PoolBalance::get();
			PoolBalance::put(old_pool_balance + amount);
			<Balances<T>>::insert(origin_account, origin_balance - amount);
		}


		/// Manta's private transfer function that moves values from two
		/// sender's private tokens into two receiver tokens. A proof is required to
		/// make sure that this transaction is valid.
		/// Neither the values nor the identities is leaked during this process.
		#[weight = 0]
		fn manta_transfer(origin,
			sender_data_1: [u8; 96],
			sender_data_2: [u8; 96],
			receiver_data_1: [u8; 80],
			receiver_data_2: [u8; 80],
			proof: [u8; 192],
		) {

			let sender_data_1 = SenderData::deserialize(sender_data_1.as_ref());
			let sender_data_2 = SenderData::deserialize(sender_data_2.as_ref());
			let receiver_data_1 = ReceiverData::deserialize(receiver_data_1.as_ref());
			let receiver_data_2 = ReceiverData::deserialize(receiver_data_2.as_ref());
			ensure!(Self::is_init(), <Error<T>>::BasecoinNotInit);
			let origin = ensure_signed(origin)?;

			let hash_param = HashParam::deserialize(HASH_PARAM_BYTES.as_ref());
			let hash_param_checksum_local = hash_param.get_checksum();


			// get the parameter checksum from the ledger
			let hash_param_checksum = HashParamChecksum::get();
			ensure!(
				hash_param_checksum_local == hash_param_checksum,
				<Error<T>>::ParamFail
			);
			// todo: checksum ZKP verification eky


			// check if vn_old already spent
			let mut sn_list = VNList::get();
			ensure!(
				!sn_list.contains(&sender_data_1.sn),
				<Error<T>>::MantaCoinSpent
			);
			ensure!(
				!sn_list.contains(&sender_data_2.sn),
				<Error<T>>::MantaCoinSpent
			);
			sn_list.push(sender_data_1.sn);
			sn_list.push(sender_data_2.sn);

			// get the ledger state from the ledger
			// and check the validity of the state
			let mut coin_shards = CoinShards::get();
			ensure!(
				coin_shards.check_root(&sender_data_1.root),
				<Error<T>>::InvalidLedgerState
			);
			ensure!(
				coin_shards.check_root(&sender_data_2.root),
				<Error<T>>::InvalidLedgerState
			);
			// check the commitment are not in the list already
			ensure!(
				!coin_shards.exist(&receiver_data_1.cm),
				<Error<T>>::MantaCoinExist
			);
			ensure!(
				!coin_shards.exist(&receiver_data_2.cm),
				<Error<T>>::MantaCoinExist
			);

			// update coin list
			// with sharding, there is no point to batch update
			// since the commitments are likely to go to different shards
			coin_shards.update(&receiver_data_1.cm, hash_param.clone());
			coin_shards.update(&receiver_data_2.cm, hash_param);

			// get the verification key from the ledger
			let transfer_vk_bytes = TransferZKPKey::get();

			// check validity of zkp
			ensure!(
				crypto::manta_verify_transfer_zkp(
					transfer_vk_bytes,
					proof,
					&sender_data_1,
					&sender_data_2,
					&receiver_data_1,
					&receiver_data_2),
				<Error<T>>::ZkpFail,
			);

			// TODO: revisit replay attack here

			// update ledger storage
			let mut enc_value_list = EncValueList::get();
			enc_value_list.push(receiver_data_1.cipher);
			enc_value_list.push(receiver_data_2.cipher);

			Self::deposit_event(RawEvent::PrivateTransferred(origin));
			CoinShards::put(coin_shards);
			VNList::put(sn_list);
			EncValueList::put(enc_value_list);
		}


		/// Manta's reclaim function that moves values from two
		/// sender's private tokens into a receiver public account, and a private token.
		/// A proof is required to
		/// make sure that this transaction is valid.
		/// Neither the values nor the identities is leaked during this process;
		/// except for the reclaimed amount.
		/// At the moment, the reclaimed amount goes directly to `origin` account.
		/// __TODO__: shall we use a different receiver rather than `origin`?
		#[weight = 0]
		fn reclaim(origin,
			amount: u64,
			sender_data_1: [u8; 96],
			sender_data_2: [u8; 96],
			receiver_data: [u8; 80],
			proof: [u8; 192],
		) {

			let sender_data_1 = SenderData::deserialize(sender_data_1.as_ref());
			let sender_data_2 = SenderData::deserialize(sender_data_2.as_ref());
			let receiver_data = ReceiverData::deserialize(receiver_data.as_ref());

			let origin = ensure_signed(origin)?;
			let origin_account = origin.clone();
			let origin_balance = <Balances<T>>::get(&origin);
			ensure!(Self::is_init(), <Error<T>>::BasecoinNotInit);

			let hash_param = HashParam::deserialize(HASH_PARAM_BYTES.as_ref());
			let hash_param_checksum_local = hash_param.get_checksum();


			// get the parameter checksum from the ledger
			let hash_param_checksum = HashParamChecksum::get();
			ensure!(
				hash_param_checksum_local == hash_param_checksum,
				<Error<T>>::MintFail
			);
			// todo: checksum ZKP verification eky

			// check the balance is greater than amount
			let mut pool = PoolBalance::get();
			ensure!(pool>=amount, <Error<T>>::PoolOverdrawn);
			pool -= amount;

			// check if sn_old already spent
			let mut sn_list = VNList::get();
			ensure!(
				!sn_list.contains(&sender_data_1.sn),
				<Error<T>>::MantaCoinSpent
			);
			ensure!(
				!sn_list.contains(&sender_data_2.sn),
				<Error<T>>::MantaCoinSpent
			);
			sn_list.push(sender_data_1.sn);
			sn_list.push(sender_data_2.sn);

			// get the coin list
			let mut coin_shards = CoinShards::get();

			// get the verification key from the ledger
			let reclaim_vk_bytes = ReclaimZKPKey::get();

			// get the ledger state from the ledger
			// and check the validity of the state
			ensure!(
				coin_shards.check_root(&sender_data_1.root),
				<Error<T>>::InvalidLedgerState
			);
			ensure!(
				coin_shards.check_root(&sender_data_2.root),
				<Error<T>>::InvalidLedgerState
			);
			// check the commitment are not in the list already
			ensure!(
				!coin_shards.exist(&receiver_data.cm),
				<Error<T>>::MantaCoinSpent
			);


			// check validity of zkp
			ensure!(
				crypto::manta_verify_reclaim_zkp(
					reclaim_vk_bytes,
					amount,
					proof,
					&sender_data_1,
					&sender_data_2,
					&receiver_data),
				<Error<T>>::ZkpFail,
			);

			// TODO: revisit replay attack here

			// update ledger storage
			let mut enc_value_list = EncValueList::get();
			enc_value_list.push(receiver_data.cipher);


			coin_shards.update(&receiver_data.cm, hash_param);
			CoinShards::put(coin_shards);

			Self::deposit_event(RawEvent::PrivateReclaimed(origin));
			VNList::put(sn_list);
			PoolBalance::put(pool);
			EncValueList::put(enc_value_list);
			<Balances<T>>::insert(origin_account, origin_balance + amount);
		}

	}
}

decl_event! {
	pub enum Event<T> where
		<T as frame_system::Config>::AccountId,
	{
		/// The asset was issued. \[owner, total_supply\]
		Issued(AccountId, u64),
		/// The asset was transferred. \[from, to, amount\]
		Transferred(AccountId, AccountId, u64),
		/// The asset was minted to private
		Minted(AccountId, u64),
		/// Private transfer
		PrivateTransferred(AccountId),
		/// The assets was reclaimed
		PrivateReclaimed(AccountId),
	}
}

decl_error! {
	/// Error messages.
	pub enum Error for Module<T: Config> {
		/// This token has already been initiated
		AlreadyInitialized,
		/// Transfer when not initialized
		BasecoinNotInit,
		/// Transfer amount should be non-zero
		AmountZero,
		/// Account balance must be greater than or equal to the transfer amount
		BalanceLow,
		/// Balance should be non-zero
		BalanceZero,
		/// Mint failure
		MintFail,
		/// MantaCoin exist
		MantaCoinExist,
		/// MantaCoin does not exist
		MantaNotCoinExist,
		/// MantaCoin already spend
		MantaCoinSpent,
		/// ZKP verification failed
		ZkpFail,
		/// invalid ledger state
		InvalidLedgerState,
		/// Pool overdrawn
		PoolOverdrawn,
		/// Invalid parameters
		ParamFail,
	}
}

decl_storage! {
	trait Store for Module<T: Config> as Assets {
		/// The number of units of assets held by any given account.
		pub Balances: map hasher(blake2_128_concat) T::AccountId => u64;

		/// The total unit supply of the asset.
		pub TotalSupply get(fn total_supply): u64;

		/// Returns a boolean: is this token already initialized (can only initiate once)
		pub Init get(fn is_init): bool;

		/// List of _void number_s.
		/// A void number is also known as a `serial number` in other protocols.
		/// Each coin has a unique void number, and if this number is revealed,
		/// the coin is voided.
		/// The ledger maintains a list of all void numbers.
		pub VNList get(fn vn_list): Vec<[u8; 32]>;

		/// List of Coins that has ever been created.
		/// We employ a sharding system to host all the coins
		/// for better concurrency.
		pub CoinShards get(fn coin_shards): Shards;

		/// List of encrypted values.
		pub EncValueList get(fn enc_value_list): Vec<[u8; 16]>;

		/// The balance of all minted coins.
		pub PoolBalance get(fn pool_balance): u64;

		/// The checksum of hash parameter.
		pub HashParamChecksum get(fn hash_param_checksum): [u8; 32];

		/// The checksum of commitment parameter.
		pub CommitParamChecksum get(fn commit_param_checksum): [u8; 32];

		/// The verification key for zero-knowledge proof for transfer protocol.
		/// At the moment we are storing the whole serialized key
		/// in the blockchain storage.
		pub TransferZKPKey get(fn transfer_zkp_vk): Vec<u8>;

		/// The verification key for zero-knowledge proof for reclaim protocol.
		/// At the moment we are storing the whole serialized key
		/// in the blockchain storage.
		pub ReclaimZKPKey get(fn reclaim_zkp_vk): Vec<u8>;
	}
}

// The main implementation block for the module.
impl<T: Config> Module<T> {
	// Public immutables

	/// Get the asset `id` balance of `who`.
	pub fn balance(who: T::AccountId) -> u64 {
		<Balances<T>>::get(who)
	}
}
