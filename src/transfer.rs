use crate::manta_token::*;
use crate::param::*;
use ark_crypto_primitives::{
    commitment::pedersen::Randomness,
    prf::{blake2s::constraints::Blake2sGadget, PRFGadget},
    CommitmentGadget, PathVar,
};
use ark_ed_on_bls12_381::{EdwardsProjective, Fq, Fr};
use ark_r1cs_std::{alloc::AllocVar, prelude::*};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::CanonicalDeserialize;
use ark_std::vec::Vec;

// =============================
// circuit for the following statements
// 1. both sender's and receiver's coins are well-formed
//  1.1 k = com(pk||rho, r)
//  1.2 cm = com(v||k, s)
// where both k and cm are public
// 2. address and the secret key derives public key
//  sender.pk = PRF(sender_sk, [0u8;32])
// 3. sender's commitment is in List_all
//  NOTE: we de not need to prove that sender's sn is not in List_USD
//        this can be done in the public
// 4. sender's and receiver's value are the same
// =============================
#[derive(Clone)]
pub struct TransferCircuit {
    // param
    pub commit_param: MantaCoinCommitmentParam,
    pub hash_param: HashParam,

    // sender
    pub sender_coin: MantaCoin,
    pub sender_pub_info: MantaCoinPubInfo,
    pub sender_priv_info: MantaCoinPrivInfo,

    // receiver
    pub receiver_coin: MantaCoin,
    pub receiver_pub_info: MantaCoinPubInfo,

    // ledger
    pub list: Vec<[u8; 32]>,
}

impl ConstraintSynthesizer<Fq> for TransferCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fq>) -> Result<(), SynthesisError> {
        // 1. both sender's and receiver's coins are well-formed
        //  k = com(pk||rho, r)
        //  cm = com(v||k, s)

        // parameters
        let parameters_var = MantaCoinCommitmentParamVar::new_input(
            ark_relations::ns!(cs, "gadget_parameters"),
            || Ok(&self.commit_param),
        )
        .unwrap();

        token_well_formed_circuit_helper(
            true,
            &parameters_var,
            &self.sender_coin,
            &self.sender_pub_info,
            self.sender_priv_info.value,
            cs.clone(),
        );

        token_well_formed_circuit_helper(
            false,
            &parameters_var,
            &self.receiver_coin,
            &self.receiver_pub_info,
            self.sender_priv_info.value,
            cs.clone(),
        );

        // 2. address and the secret key derives public key
        //  sender.pk = PRF(sender_sk, [0u8;32])
        //  sender.sn = PRF(sender_sk, rho)
        prf_circuit_helper(
            true,
            &self.sender_priv_info.sk,
            &[0u8; 32],
            &self.sender_pub_info.pk,
            cs.clone(),
        );
        prf_circuit_helper(
            false,
            &self.sender_priv_info.sk,
            &self.sender_pub_info.rho,
            &self.sender_priv_info.sn,
            cs.clone(),
        );

        // 3. sender's commitment is in List_all
        merkle_membership_circuit_proof(
            &self.hash_param,
            &self.sender_coin.cm_bytes,
            &self.list,
            cs,
        );

        // 4. sender's and receiver's value are the same
        // this is implied since a same value goes to both
        // sender and receiver token_well_formed circuit

        Ok(())
    }
}

// =============================
// circuit for the following statements
// 1. k = com(pk||rho, r)
// 2. cm = com(v||k, s)
// for the sender, the cm is hidden and k is public
// for the receiver, both are public
// =============================
pub(crate) fn token_well_formed_circuit_helper(
    is_sender: bool,
    parameters_var: &MantaCoinCommitmentParamVar,
    coin: &MantaCoin,
    pub_info: &MantaCoinPubInfo,
    value: u64,
    cs: ConstraintSystemRef<Fq>,
) {
    // =============================
    // statement 1: k = com(pk||rho, r)
    // =============================
    let input: Vec<u8> = [pub_info.pk.as_ref(), pub_info.rho.as_ref()].concat();
    let mut input_var = Vec::new();
    for byte in &input {
        input_var.push(UInt8::new_witness(cs.clone(), || Ok(*byte)).unwrap());
    }

    // openning
    let r = Fr::deserialize(pub_info.r.as_ref()).unwrap();
    let r = Randomness::<EdwardsProjective>(r);
    let randomness_var = MantaCoinCommitmentOpenVar::new_witness(
        ark_relations::ns!(cs, "gadget_randomness"),
        || Ok(&r),
    )
    .unwrap();

    // commitment
    let result_var =
        MantaCoinCommitmentSchemeVar::commit(&parameters_var, &input_var, &randomness_var).unwrap();

    // circuit to compare the commited value with supplied value
    let k = MantaCoinCommitmentOutput::deserialize(pub_info.k.as_ref()).unwrap();
    let commitment_var2 = MantaCoinCommitmentOutputVar::new_input(
        ark_relations::ns!(cs, "gadget_commitment"),
        || Ok(k),
    )
    .unwrap();
    result_var.enforce_equal(&commitment_var2).unwrap();

    // =============================
    // statement 2: cm = com(v||k, s)
    // =============================
    let input: Vec<u8> = [value.to_le_bytes().as_ref(), pub_info.k.as_ref()].concat();
    let mut input_var = Vec::new();
    for byte in &input {
        input_var.push(UInt8::new_witness(cs.clone(), || Ok(*byte)).unwrap());
    }

    // openning
    let s = Randomness::<EdwardsProjective>(Fr::deserialize(pub_info.s.as_ref()).unwrap());
    let randomness_var = MantaCoinCommitmentOpenVar::new_witness(
        ark_relations::ns!(cs, "gadget_randomness"),
        || Ok(&s),
    )
    .unwrap();

    // commitment
    let result_var: MantaCoinCommitmentOutputVar =
        MantaCoinCommitmentSchemeVar::commit(&parameters_var, &input_var, &randomness_var).unwrap();

    // the other commitment
    let cm: MantaCoinCommitmentOutput =
        MantaCoinCommitmentOutput::deserialize(coin.cm_bytes.as_ref()).unwrap();
    // if the commitment is from the sender, then the commitment is hidden
    // else, it is public
    let commitment_var2 = if is_sender {
        MantaCoinCommitmentOutputVar::new_witness(
            ark_relations::ns!(cs, "gadget_commitment"),
            || Ok(cm),
        )
        .unwrap()
    } else {
        MantaCoinCommitmentOutputVar::new_input(ark_relations::ns!(cs, "gadget_commitment"), || {
            Ok(cm)
        })
        .unwrap()
    };

    // circuit to compare the commited value with supplied value
    result_var.enforce_equal(&commitment_var2).unwrap();
}

/// a helper function to generate the prf circuit
///     sender.pk = PRF(sender_sk, [0u8;32])
///     sender.sn = PRF(sender_sk, rho)
/// the output pk is hidden, while sn can be public
pub(crate) fn prf_circuit_helper(
    is_output_hidden: bool,
    seed: &[u8; 32],
    input: &[u8; 32],
    output: &[u8; 32],
    cs: ConstraintSystemRef<Fq>,
) {
    // step 1. Allocate seed
    let seed_var = Blake2sGadget::new_seed(cs.clone(), &seed);

    // step 2. Allocate inputs
    let input_var = UInt8::new_witness_vec(ark_relations::ns!(cs, "declare_input"), input).unwrap();

    // step 3. Allocate evaluated output
    let output_var = Blake2sGadget::evaluate(&seed_var, &input_var).unwrap();

    // step 4. Actual output
    let actual_out_var = if is_output_hidden {
        <Blake2sGadget as PRFGadget<_, Fq>>::OutputVar::new_witness(
            ark_relations::ns!(cs, "declare_output"),
            || Ok(output),
        )
        .unwrap()
    } else {
        <Blake2sGadget as PRFGadget<_, Fq>>::OutputVar::new_input(
            ark_relations::ns!(cs, "declare_output"),
            || Ok(output),
        )
        .unwrap()
    };

    // step 5. compare the outputs
    output_var.enforce_equal(&actual_out_var).unwrap();
}

pub(crate) fn merkle_membership_circuit_proof(
    param: &HashParam,
    cm: &[u8; 32],
    list: &[[u8; 32]],
    cs: ConstraintSystemRef<Fq>,
) {
    // check if cm is in or not; if cm is not in, panic!
    let index = list.iter().position(|x| x == cm).unwrap();

    // build the merkle tree
    let tree = LedgerMerkleTree::new(param.clone(), &list).unwrap();
    let merkle_root = tree.root();
    let path = tree.generate_proof(index, &cm).unwrap();

    // Allocate Merkle Tree Root
    let root_var =
        HashOutputVar::new_input(ark_relations::ns!(cs, "new_digest"), || Ok(merkle_root)).unwrap();

    // Allocate Parameters for CRH
    let param_var =
        HashParamVar::new_constant(ark_relations::ns!(cs, "new_parameter"), param).unwrap();

    // Allocate Merkle Tree Path
    let membership_var =
        PathVar::<_, HashVar, _>::new_witness(ark_relations::ns!(cs, "new_witness"), || Ok(&path))
            .unwrap();

    // Allocate Leaf
    let leaf_var = UInt8::new_witness_vec(ark_relations::ns!(cs, "commitment"), cm).unwrap();
    let leaf_var: &[_] = leaf_var.as_slice();

    // check membership
    membership_var
        .check_membership(&param_var, &root_var, &leaf_var)
        .unwrap()
        .enforce_equal(&Boolean::TRUE)
        .unwrap();
}
