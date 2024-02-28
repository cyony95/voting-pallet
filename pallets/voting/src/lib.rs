//! # Quadratic Voting Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! The Quadratic Voting pallet handles the administration of voting mechanisms
//! using a quadratic approach.
//!
//! There is one pool that the proposals are added into that the voter can choose
//! to vote on.
//!
//! The proposals have a configurable duration that starts from the moment the
//! proposal is created and is counted in block numbers. It has to be manually closed.
//!
//! The voters will vote in approval ("Aye") or rejection ("Nay"), choosing how many votes
//! they want to add to their choice and locking the square of the votes as tokens.
//!
//! The voters have the chance to unlock their tokens after the proposal has been closed.
//!
//! ### Terminology
//!
//! - **Lock Period:** A period of time after proposal enactment that the tokens of _winning_ voters
//! will be locked.
//! - **Conviction:** An indication of a voter's strength of belief in their vote. An increase
//! in conviction indicates that a token holder is willing to lock the square of their votes as
//! tokens.
//! - **Vote:** A value that can either be in approval ("Aye") or rejection ("Nay") of a particular
//! referendum.
//! - **Proposal:** A submission to the chain that represents an action that a proposer (either an
//! account or an external origin) suggests that the system adopt.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! #### Public
//!
//! These calls can be made from any externally held account capable of creating
//! a signed extrinsic.
//!
//! - 'end_vote' - Will end the vote if the time allocation has expired.
//!
//! #### Registered users
//!
//! These calls can only be made by an account that has been registered into the pool.
//!
//! - `propose` - Submits a proposal, represented as a hash. Requires the a registered voter.
//! - `vote` - Votes for a proposal, either the vote is "Aye" to enact the proposal or "Nay" to keep
//!   the status quo. The number of votes scales quadratically with the tokens frozen as a deposit.
//! - 'claim frozen tokens' The voter can claim the frozen tokens used for a proposal, after the
//!   proposal ends.
//!
//! #### Root
//!
//! - 'register voters' - Registers an account into a pool of voters. Requires sudo.

#![cfg_attr(not(feature = "std"), no_std)]
use frame_support::{
	dispatch::Vec,
	pallet_prelude::*,
	sp_runtime::traits::{AtLeast32BitUnsigned, CheckedAdd, CheckedMul, CheckedSub, Convert, Hash},
	traits::{
		fungible,
		fungible::{InspectFreeze, MutateFreeze},
	},
};
use frame_system::pallet_prelude::BlockNumberFor;
pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use core::cmp::Ordering;
	use frame_support::{
		sp_runtime::traits::{One, Zero},
		BoundedVec,
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching freeze reason.
		type RuntimeFreezeReason: From<FreezeReason>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId, Id = Self::RuntimeFreezeReason>;

		/// A helper to convert a block number to a balance type.
		type BlockNumberToBalance: Convert<BlockNumberFor<Self>, BalanceOf<Self>>;

		/// Max number of votes.
		/// Configurable in the runtime config.
		#[pallet::constant]
		type MaxVotes: Get<u32>;

		/// Proposal duration measured in block numbers.
		/// The proposal cannot be closed before this many blocks have been added since the proposal
		/// started. The proposal can be closed at any time after that.
		/// Configurable in the runtime config.
		#[pallet::constant]
		type ProposalDuration: Get<BlockNumberFor<Self>>;

		/// The proposal index type.
		/// The concrete type is configurable in the runtime config.
		type ProposalId: AtLeast32BitUnsigned
			+ Copy
			+ Eq
			+ PartialEq
			+ Parameter
			+ MaxEncodedLen
			+ Default
			+ One
			+ CheckedAdd;
	}

	/// Information about a created proposal.
	/// Ayes and nays are of type Balance because they represent the square root of a frozen amount
	/// of tokens.
	#[derive(Encode, Decode, Clone, MaxEncodedLen, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Proposal<T: Config> {
		pub description: T::Hash,
		pub start_block: BlockNumberFor<T>,
		pub ayes: BalanceOf<T>,
		pub nays: BalanceOf<T>,
		pub end: bool,
	}

	/// Information about a specific vote on a specific proposal from a voter.
	#[derive(Encode, Debug, Decode, Clone, MaxEncodedLen, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct UserVoteInfo<T: Config> {
		pub proposal_id: T::ProposalId,
		pub aye: bool,
		pub votes: BalanceOf<T>,
	}

	/// A reason for freezing funds.
	#[pallet::composite_enum]
	pub enum FreezeReason {
		#[codec(index = 0)]
		AccountDeposit,
	}

	/// A map of all the accounts that have been registered to vote.
	#[pallet::storage]
	pub type RegisteredAccounts<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, bool>;

	/// A value that increments with the number of proposals created.
	/// It holds the next available id.
	#[pallet::storage]
	pub type ProposalIndex<T: Config> = StorageValue<_, T::ProposalId, ValueQuery>;

	/// A map of all the proposals.
	#[pallet::storage]
	pub type ProposalPool<T: Config> = StorageMap<_, Blake2_128Concat, T::ProposalId, Proposal<T>>;

	/// A map of the voting history of every account. It only keeps track for active proposals or
	/// if the user hasn't claimed back the tokens after a proposal has ended.
	#[pallet::storage]
	pub type VotingHistory<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, BoundedVec<UserVoteInfo<T>, T::MaxVotes>>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Voter registered.
		VoterRegistered { voter: T::AccountId },
		/// Proposal was successfully created.
		ProposalCreated { proposal_id: T::ProposalId },
		/// Vote successfully added.
		VoteAddedTo { proposal_id: T::ProposalId, votes: BalanceOf<T> },
		/// Vote has finished. Proposal was accepted by the community.
		ProposalResultAye { proposal_id: T::ProposalId },
		/// Vote has finished. Proposal was not accepted by the community.
		ProposalResultNay { proposal_id: T::ProposalId },
		/// Vote has finished but it is a tie.
		ProposalResultTie { proposal_id: T::ProposalId },
		/// Tokens have been unlocked.
		TokensUnlocked,
		/// Amount of tokens frozen for this proposal is smaller than the max frozen amount.
		NoTokensUnlocked,
		/// Vote removed from the proposal by specifiying a zero amount of votes.
		VoteRemovedOrCancelled { proposal_id: T::ProposalId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// No proposal with the provided ID exists.
		ProposalDoesNotExist,
		/// User is not registered in the identity system.
		NotRegistered,
		/// Operation overflowed.
		Overflow,
		/// Operation underflowed.
		Underflow,
		/// Insufficient funds to cast that many votes.
		InsufficientFunds,
		/// Too many votes on too many proposals for this account.
		TooManyVotes,
		/// Voting period is not over. Vote is still in progress.
		VotingPeriodNotOver,
		/// Vote has already ended.
		VoteAlreadyEnded,
		/// Voter already registered.
		VoterAlreadyRegistered,
		/// No votes from this account found for the specified proposal.
		NoVotes,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A dispatchable that registers voters.
		///
		/// The dispatch origin of this call must be Sudo.
		///
		/// - `voter`: the AccountId to be registered in the voter pool.
		///
		/// Emits `VoterRegistered { voter }`.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::default())]
		pub fn register_voters(origin: OriginFor<T>, voter: T::AccountId) -> DispatchResult {
			ensure_root(origin)?;

			RegisteredAccounts::<T>::get(&voter)
				.map_or(Ok(()), |_| Err(Error::<T>::VoterAlreadyRegistered))?;

			RegisteredAccounts::<T>::insert(&voter, true);

			Self::deposit_event(Event::VoterRegistered { voter });

			Ok(())
		}

		/// A dispatchable that creates proposals.
		///
		/// The dispatch origin of this call must be Signed and the sender must
		/// be a registered voter.
		///
		/// - `proposal_hash`: The hash of the proposal preimage.
		///
		/// Emits `ProposalCreated { proposal_id }`
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::default())]
		pub fn make_proposal(
			origin: OriginFor<T>,
			proposal_description: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			RegisteredAccounts::<T>::get(&who).ok_or(Error::<T>::NotRegistered)?;

			let proposal = Proposal::<T> {
				description: <T as frame_system::Config>::Hashing::hash(&proposal_description),
				start_block: Self::get_current_block_number(),
				ayes: BalanceOf::<T>::zero(),
				nays: BalanceOf::<T>::zero(),
				end: false,
			};

			// ValueQuery makes sure it returns 0 if no proposals exist.
			let proposal_id = <ProposalIndex<T>>::get();
			<ProposalPool<T>>::insert(proposal_id, proposal);

			Self::deposit_event(Event::ProposalCreated { proposal_id });

			// Prepare the next proposal id.
			let new_proposal_id =
				proposal_id.checked_add(&T::ProposalId::one()).ok_or(Error::<T>::Overflow)?;
			<ProposalIndex<T>>::set(new_proposal_id);

			Ok(())
		}

		/// A dispatchable that casts a vote on a specific proposal.
		///
		/// The dispatch origin of this call must be Signed and the sender must
		/// be a registered voter.
		///
		/// - `votes`: The number of votes.
		/// - `aye': true for 'Aye', False for 'Nay'.
		/// - `proposal_id`: The id of the proposal to vote on.
		///
		/// Emits `VoteAddedTo { proposal_id, votes }` in case the vote has been added.
		/// Emits `VoteRemovedOrCanceled { proposal_id }` in case the vote has been canceled or
		/// removed.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::default())]
		pub fn vote(
			origin: OriginFor<T>,
			votes: BalanceOf<T>,
			aye: bool,
			proposal_id: T::ProposalId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			RegisteredAccounts::<T>::get(&who).ok_or(Error::<T>::NotRegistered)?;

			// Check if the proposal exists.
			let mut proposal =
				<ProposalPool<T>>::get(proposal_id).ok_or(Error::<T>::ProposalDoesNotExist)?;

			// Check the proposal hasn't ended.
			ensure!(!proposal.end, Error::<T>::VoteAlreadyEnded);

			let required_tokens = votes.checked_mul(&votes).ok_or(Error::<T>::Overflow)?;
			let account_balance =
				<T::NativeBalance as fungible::Inspect<T::AccountId>>::total_balance(&who);

			// Make sure the voter has enough tokens to vote.
			ensure!(account_balance >= required_tokens, Error::<T>::InsufficientFunds);

			// Prepare to update the voter's voting history.
			let mut new_voting_history = BoundedVec::new();
			let user_vote = UserVoteInfo { aye, proposal_id, votes };

			// Check if the voter has voted before on this proposal and removes his votes.
			if let Some((index, mut voting_history)) =
				Self::find_existing_vote(who.clone(), proposal_id)
			{
				// Remove the votes from the proposal.
				Self::remove_votes_from_proposal(
					&mut proposal,
					voting_history[index].aye,
					voting_history[index].votes,
				)?;

				// Remove the votes from the voting history.
				voting_history.remove(index);

				VotingHistory::<T>::insert(who.clone(), voting_history.clone());

				// Unfreeze the tokens if necessary.
				Self::unfreeze(who.clone(), &mut voting_history)?;

				new_voting_history = voting_history;
			}

			// Then act like he is a new voter and add his new vote.
			// If the amount of votes is 0, do nothing.
			if votes == BalanceOf::<T>::default() {
				Self::deposit_event(Event::VoteRemovedOrCancelled { proposal_id });
				return Ok(())
			}

			Self::freeze(who, user_vote, &mut new_voting_history, required_tokens)?;

			Self::add_votes_to_proposal(&mut proposal, aye, votes)?;

			<ProposalPool<T>>::insert(proposal_id, proposal);

			Self::deposit_event(Event::VoteAddedTo { proposal_id, votes });
			Ok(())
		}

		/// A dispatchable that ends the vote if the voting period is finished.
		///
		/// The dispatch origin of this call must be Signed and the sender can
		/// be anyone.
		///
		/// - `votes`: The number of votes.
		/// - `proposal_id`: The id of the proposal to close.
		///
		/// Emits `Event::ProposalResultAye { proposal_id }` in case the proposal is accepted.
		/// Emits `Event::ProposalResultNay { proposal_id }` in case the proposal is rejected.
		/// Emits `Event::ProposalResultTie { proposal_id }` in case the vote ends in a tie.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::default())]
		pub fn end_vote(origin: OriginFor<T>, proposal_id: T::ProposalId) -> DispatchResult {
			ensure_signed(origin)?;

			let mut proposal =
				<ProposalPool<T>>::get(proposal_id).ok_or(Error::<T>::ProposalDoesNotExist)?;

			// Check the proposal hasn't ended.
			ensure!(!proposal.end, Error::<T>::VoteAlreadyEnded);

			// Convert both block numbers to balances so we can compare them
			let start_block = Self::convert_block_number_to_balance(proposal.start_block);
			let current_block =
				Self::convert_block_number_to_balance(Self::get_current_block_number());

			// Check if the proposal time has ended.
			Self::proposal_ended(start_block, current_block, &mut proposal)?;

			// Calculate the outcome of the vote.
			match proposal.ayes.cmp(&proposal.nays) {
				Ordering::Greater => Self::deposit_event(Event::ProposalResultAye { proposal_id }),
				Ordering::Less => Self::deposit_event(Event::ProposalResultNay { proposal_id }),
				Ordering::Equal => Self::deposit_event(Event::ProposalResultTie { proposal_id }),
			}

			// Close the proposal.
			<ProposalPool<T>>::insert(proposal_id, proposal);
			Ok(())
		}

		/// A dispatchable that allows voters to reclaim their frozen tokens after a proposal has
		/// been closed.
		///
		/// The dispatch origin of this call must be Signed and the sender must
		/// be a registered voter.
		///
		/// - `proposal_id`: The id of the proposal to close.
		///
		/// Emits `Event::TokensUnlocked` in case there are eligible tokens.
		/// Emits `Event::NoTokensUnlocked` in case there aren't any eligible tokens.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::default())]
		pub fn claim_frozen_tokens(
			origin: OriginFor<T>,
			proposal_id: T::ProposalId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			RegisteredAccounts::<T>::get(&who).ok_or(Error::<T>::NotRegistered)?;

			// Check if the proposal exists.
			let proposal =
				<ProposalPool<T>>::get(proposal_id).ok_or(Error::<T>::ProposalDoesNotExist)?;

			// Check the proposal has ended.
			ensure!(proposal.end, Error::<T>::VotingPeriodNotOver);

			// Check if there are votes for this proposal from this account.
			let mut voting_history =
				VotingHistory::<T>::get(who.clone()).ok_or(Error::<T>::NoVotes)?;

			// Get the highest amount of votes from this account's voting history.
			let (index, max_freeze_proposal) = voting_history
				.iter()
				.enumerate()
				.max_by_key(|(_, item)| item.proposal_id)
				.ok_or(Error::<T>::NoVotes)?; // This should never return an error.

			// If the amount locked by this proposal is not the highest, don't do anything.
			if !max_freeze_proposal.proposal_id.eq(&proposal_id) {
				Self::deposit_event(Event::NoTokensUnlocked);
				return Ok(());
			}

			// Remove the votes from the account voting history.
			voting_history.remove(index);
			VotingHistory::<T>::insert(who.clone(), voting_history.clone());

			Self::unfreeze(who, &mut voting_history)?;

			Self::deposit_event(Event::TokensUnlocked);

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// How to get the current block from the FRAME System Pallet.
	pub fn get_current_block_number() -> BlockNumberFor<T> {
		frame_system::Pallet::<T>::block_number()
	}

	/// How to convert a block number to the balance type.
	pub fn convert_block_number_to_balance(block_number: BlockNumberFor<T>) -> BalanceOf<T> {
		T::BlockNumberToBalance::convert(block_number)
	}

	/// Remove a number of aye or nay votes from the proposal.
	fn remove_votes_from_proposal(
		proposal: &mut Proposal<T>,
		aye: bool,
		votes: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		match aye {
			true => {
				proposal.ayes = proposal.ayes.checked_sub(&votes).ok_or(Error::<T>::Underflow)?;
			},
			false => {
				proposal.nays = proposal.nays.checked_sub(&votes).ok_or(Error::<T>::Underflow)?;
			},
		}

		Ok(())
	}

	/// Add a number of aye or nay votes to the proposal.
	fn add_votes_to_proposal(
		proposal: &mut Proposal<T>,
		aye: bool,
		votes: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		match aye {
			true => {
				proposal.ayes = proposal.ayes.checked_add(&votes).ok_or(Error::<T>::Overflow)?;
			},
			false => {
				proposal.nays = proposal.nays.checked_add(&votes).ok_or(Error::<T>::Overflow)?;
			},
		}

		Ok(())
	}

	/// Freeze tokens if this is the highest amount to freeze.
	/// Applies also if this is the first freeze on this account.
	/// It doesn't check if the account has enough tokens, so that check needs to be done
	/// beforehand!
	fn freeze(
		who: T::AccountId,
		user_vote: UserVoteInfo<T>,
		new_voting_history: &mut BoundedVec<UserVoteInfo<T>, T::MaxVotes>,
		required_tokens: BalanceOf<T>,
	) -> Result<(), DispatchError> {
		// Check if there are other votes for this account.
		if let Some(mut voting_history) = VotingHistory::<T>::get(who.clone()) {
			// Update his voting history.
			voting_history.try_push(user_vote).map_err(|_| Error::<T>::TooManyVotes)?;
			VotingHistory::<T>::insert(who.clone(), voting_history);
		} else {
			new_voting_history.try_push(user_vote).map_err(|_| Error::<T>::TooManyVotes)?;
			VotingHistory::<T>::insert(who.clone(), new_voting_history.clone());
			// The account has no other freezes.
			T::NativeBalance::set_freeze(
				&FreezeReason::AccountDeposit.into(),
				&who,
				required_tokens,
			)?;
		}
		// If this is the highest freeze until now, set this as the new freeze amount.
		if required_tokens >
			T::NativeBalance::balance_frozen(&FreezeReason::AccountDeposit.into(), &who)
		{
			T::NativeBalance::set_freeze(
				&FreezeReason::AccountDeposit.into(),
				&who,
				required_tokens,
			)?;
		}

		Ok(())
	}

	/// Removes freezes from the specified account considering the passed voting_history.
	/// If there s only one vote, it will thaw the frozen amount.
	/// If there are multiple, it will set the freeze to the next max value.
	fn unfreeze(
		who: T::AccountId,
		voting_history: &mut BoundedVec<UserVoteInfo<T>, T::MaxVotes>,
	) -> Result<(), DispatchError> {
		// Check if that was the only vote and free everything or just set the freeze to the
		// next max value.
		if let Some((_, max_freeze_proposal)) =
			voting_history.iter().enumerate().max_by_key(|(_, item)| item.proposal_id)
		{
			T::NativeBalance::set_freeze(
				&FreezeReason::AccountDeposit.into(),
				&who,
				max_freeze_proposal
					.votes
					.checked_mul(&max_freeze_proposal.votes)
					.ok_or(Error::<T>::Overflow)?,
			)?;
		} else {
			T::NativeBalance::thaw(&FreezeReason::AccountDeposit.into(), &who)?;
		}

		Ok(())
	}

	// Checks if the proposal has ended.
	// If the time has passed, it will update the proposal's end field to true.
	fn proposal_ended(
		start_block: BalanceOf<T>,
		current_block: BalanceOf<T>,
		proposal: &mut Proposal<T>,
	) -> Result<(), DispatchError> {
		(start_block
			.checked_add(&Self::convert_block_number_to_balance(T::ProposalDuration::get()))
			.ok_or(Error::<T>::Overflow)
			.and_then(|result| {
				if result > current_block {
					Err(Error::<T>::VotingPeriodNotOver)
				} else {
					// Close the proposal.
					proposal.end = true;
					Ok(())
				}
			}))?;

		Ok(())
	}

	// Checks if there is a vote for this proposal and returns information about it.
	// If no vote exists, returns None.
	fn find_existing_vote(
		who: T::AccountId,
		proposal_id: T::ProposalId,
	) -> Option<(usize, BoundedVec<UserVoteInfo<T>, T::MaxVotes>)> {
		if let Some(voting_history) = VotingHistory::<T>::get(who) {
			if let Some((index, _)) = voting_history
				.iter()
				.enumerate()
				.find(|(_, item)| item.proposal_id.eq(&proposal_id))
			{
				return Some((index, voting_history));
			}
		}
		None
	}
}
