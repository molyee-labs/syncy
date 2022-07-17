#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{account, benchmarks, whitelist_account};
use frame_system::{Call as SystemCall, RawOrigin};
use sp_runtime::traits::Bounded;

use crate::Pallet as StakeVoting;
use core::ops::Range;
use deip_asset_system::*;
use frame_system::Config as SystemConfig;
use pallet_assets::Config as AssetsConfig;
use pallet_deip_f_nft::Config as DeipNftConfig;
use pallet_deip_f_nft::Pallet as DeipNft;
use sp_runtime::traits::Hash;
use sp_io::hashing::twox_256;
use sp_core::H160;

const SEED: u32 = 1;

fn setup_accounts<T: Config>(users: u32) -> Result<Vec<T::AccountId>, &'static str> {
    let mut holders: Vec<T::AccountId> = Vec::new();
    for i in 0..users {
        let holder = account::<T::AccountId>("user", i, SEED);
        whitelist_account!(holder);
        // Give them some balance for a possible deposit
        let balance = BalanceOf::<T>::max_value();
        T::Currency::make_free_balance_be(&holder, balance);
        holders.push(holder);
    }
    Ok(holders)
}

fn gen_call<T: Config>(size: u32) -> Result<OpaqueCall<T>, &'static str> {
    let call: <T as Config>::Call =
        SystemCall::<T>::remark { remark: vec![0; size as usize] }.into();
    let call_data = OpaqueCall::<T>::from_encoded(call.encode());
    Ok(call_data)
}

fn create_asset<T>(
    admin: &T::AccountId,
    _min: T::AssetBalance,
) -> (<T as Config>::AssetId, T::AccountId)
where
    T: Config<AssetId = <T as SystemConfig>::Hash> + DeipNftConfig,
{
    let caller: T::AccountId = admin.clone();
    let balance = BalanceOf::<T>::max_value();
    <T as Config>::Currency::make_free_balance_be(&caller, balance);
    let collection = H160::from([0u8; 20]).into();
    create_collection::<DeipNft<T>>(&caller, collection, 1u32.into()).unwrap();
    let item = <T as SystemConfig>::Hashing::hash_of(&1u32);
    DeipNft::<T>::mint_item(RawOrigin::Signed(caller.clone()).into(), collection, item).unwrap();
    (item, caller)
}

fn distribute_asset<T>(
    admin: T::AccountId,
    asset: <T as Config>::AssetId,
    amount: T::AssetBalance,
    accounts: &[T::AccountId],
) where
    T: Config<AssetId = <T as SystemConfig>::Hash, AssetBalance = T::Balance> + DeipNftConfig,
{
    assert!(!accounts.is_empty());
    assert!(!amount.is_zero());
    let total = amount * (accounts.len() as u32 + 1u32).into();
    fractionalize_item::<DeipNft<T>>(asset, &admin, total, false).unwrap();
    for u in accounts {
        transfer_fraction::<DeipNft<T>>(asset, &admin, u, amount).unwrap();
    }
}

fn create_voting<T: Config>(
    author: T::AccountId,
    asset: T::AssetId,
    start: TimeOf<T>,
    end: Option<TimeOf<T>>,
    threshold: ThresholdOf<T>,
    call_size: u32,
) -> (VotingId, VotingOf<T>) {
    let call = gen_call::<T>(call_size).unwrap();
    let call_hash = blake2_256(call.encoded());
    let voting = StakeVoting::<T>::new_voting(author.clone(), asset, start, end, threshold, call_hash);
    let id = new_id(&voting.encode());
    StakeVoting::<T>::create(
        RawOrigin::Signed(author).into(),
        id,
        asset,
        Some(start),
        end,
        threshold,
        call,
    )
    .unwrap();
    (id, voting)
}

fn now<T: Config>() -> TimeOf<T> {
    StakeVoting::<T>::timepoint()
}

fn random_range(r: Range<u32>) -> u32 {
    // TODO
    r.start
}

fn new_id(data: &[u8]) -> VotingId {
    twox_256(data)
}

fn max_weight<T: Config>(z: u32) -> Weight {
    let dbw = T::DbWeight::get();
    500000u64 + (z as u64) * 3000 + dbw.reads(15) + dbw.writes(10)
}

benchmarks! {
    where_clause {
        where T: Config<AssetId=<T as SystemConfig>::Hash,
                AssetBalance=<T as AssetsConfig>::Balance>
            + DeipNftConfig
    }
    create {
        let z in 0 .. 1000000;
        let n = random_range(2 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let asset = asset.into();
        let threshold = Threshold::Relative(T::RelativeThresholdLimit::get());
        let end = None;
        let call = gen_call::<T>(z)?;
        let call_hash = blake2_256(call.encoded());
        let caller = holders.pop().unwrap();
        let time = now::<T>();
        let voting = StakeVoting::<T>::new_voting(caller.clone(), asset, time, end, threshold, call_hash);
        let id = new_id(&voting.encode());
    }: create(RawOrigin::Signed(caller.clone()), id, asset, Some(time), end, threshold, call.clone())
    verify {
        assert!(<T as Config>::Currency::reserved_balance(&caller) > 0u32.into());
        assert_eq!(Calls::<T>::get(&voting.call_hash).map(|t| t.0), Some(call));
        assert_eq!(Votings::<T>::get(&id), Some(voting));
        assert!(States::<T>::get(&id).is_none());
        assert!(Votes::<T>::get(&(caller, asset), &id).is_none());
    }
    vote {
        let z in 0 .. 10000;
        let n = random_range(3 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let threshold = Threshold::Relative(T::RelativeThresholdLimit::get());
        let author = holders.pop().unwrap();
        let time = now::<T>();
        let (id, voting) = create_voting::<T>(author.clone(), asset.into(), time, None, threshold, z);
        let caller = holders.pop().unwrap();
    }: vote(RawOrigin::Signed(caller.clone()), id, Sign::Positive)
    verify {
        assert_eq!(Votings::<T>::get(&id), Some(voting));
        let state = State { votes: 1, yas: value, sum: value, ..Default::default() };
        assert_eq!(States::<T>::get(&id), Some(state));
        let asset: <T as Config>::AssetId = asset.into();
        let caller_vote = Votes::<T>::get(&(caller, asset), &id);
        assert!(caller_vote.is_some());
        assert_eq!(caller_vote.unwrap().0, Sign::Positive);
    }
    unvote {
        let z in 0 .. 10000;
        let n = random_range(3 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let threshold = Threshold::Relative(T::RelativeThresholdLimit::get());
        let author = holders.pop().unwrap();
        let time = now::<T>();
        let (id, voting) = create_voting::<T>(author.clone(), asset.into(), time, None, threshold, z);
        StakeVoting::<T>::vote(RawOrigin::Signed(author.clone()).into(), id, Sign::Positive).unwrap();
        let caller = holders.pop().unwrap();
        StakeVoting::<T>::vote(RawOrigin::Signed(caller.clone()).into(), id, Sign::Positive).unwrap();
    }: unvote(RawOrigin::Signed(caller.clone()), id)
    verify {
        assert_eq!(Votings::<T>::get(&id), Some(voting));
        let state = State { votes: 1, yas: value.into(), sum: value.into(), ..Default::default() };
        assert_eq!(States::<T>::get(&id), Some(state));
        let asset: <T as Config>::AssetId = asset.into();
        let author_vote = Votes::<T>::get(&(author, asset), &id);
        assert!(author_vote.is_some());
        assert_eq!(author_vote.unwrap().0, Sign::Positive);
        assert!(Votes::<T>::get(&(caller, asset), &id).is_none());
    }
    execute {
        let z in 0 .. 10000;
        let n = random_range(2 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let limit = T::RelativeThresholdLimit::get();
        let threshold = Threshold::RelativeExcept(limit / n.into());
        let caller = holders.pop().unwrap();
        let time = now::<T>();
        let (id, voting) = create_voting::<T>(caller.clone(), asset.into(), time, None, threshold, z);
        StakeVoting::<T>::vote(RawOrigin::Signed(caller.clone()).into(), id, Sign::Positive).unwrap();
        let burner = holders.pop().unwrap();
        burn_fraction::<DeipNft<T>>(asset, &burner, value).unwrap();
        let state = States::<T>::get(&id).unwrap();
        assert_eq!(state.value(), value);
        assert_eq!(state.votes, 1);
        let total = StakeVoting::<T>::total(&asset).unwrap();
        assert_eq!(total, value);
        assert!(state.is_reached(threshold, total, limit));
    }: execute(RawOrigin::Signed(caller.clone()), id, max_weight::<T>(z))
    verify {
        assert!(Votings::<T>::get(&id).is_none());
        assert!(States::<T>::get(&id).is_none());
        assert!(Calls::<T>::get(&voting.call_hash).is_none());
        let asset: <T as Config>::AssetId = asset.into();
        assert!(Votes::<T>::get(&(caller, asset), &id).is_none());
    }
    close {
        let z in 0 .. 10000;
        let n = random_range(2 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let threshold = Threshold::Relative(T::RelativeThresholdLimit::get());
        let caller = holders.pop().unwrap();
        let time = now::<T>();
        let (id, voting) = create_voting::<T>(caller.clone(), asset.into(), time, None, threshold, z);
    }: close(RawOrigin::Signed(caller.clone()), id)
    verify {
        assert!(Votings::<T>::get(&id).is_none());
        assert!(States::<T>::get(&id).is_none());
        assert!(Calls::<T>::get(&voting.call_hash).is_none());
        let asset: <T as Config>::AssetId = asset.into();
        assert!(Votes::<T>::get(&(caller, asset), &id).is_none());
    }
    /*retain_asset {
        let z in 0 .. 10000;
        let n = random_range(2 .. 100);
        let mut holders = setup_accounts::<T>(n)?;
        let min = 1u32.into();
        let (asset, admin) = create_asset::<T>(&holders[0], min);
        let value = 100u32.into();
        distribute_asset::<T>(admin, asset, value, &holders[1..]);
        let thr_value = (value + value).into();
        let threshold = Threshold::Absolute(thr_value);
        let caller = holders.pop().unwrap();
        let time = now::<T>();
        let (id, voting) = create_voting::<T>(caller.clone(), asset.into(), time, None, threshold, z);
        StakeVoting::<T>::vote(RawOrigin::Signed(caller.clone()).into(), id, Sign::Positive).unwrap();
        let approver = holders.pop().unwrap();
        StakeVoting::<T>::vote(RawOrigin::Signed(approver.clone()).into(), id, Sign::Positive).unwrap();
        // should be executed (or cancelled)
    }: retain_asset(RawOrigin::Signed(caller.clone()), asset.into())
    verify {
        assert!(Votings::<T>::get(&id).is_none());
        assert!(States::<T>::get(&id).is_none());
        let asset: <T as Config>::AssetId = asset.into();
        assert!(Votes::<T>::get(&(approver, asset), &id).is_none());
        assert!(Votes::<T>::get(&(caller, asset), &id).is_none());
    }*/

    impl_benchmark_test_suite!(StakeVoting, crate::tests::new_test_ext(), crate::tests::Test);
}
