use crate::{mock::*, Error, Event};
use frame_support::{
	assert_noop, assert_ok,
	pallet_prelude::DispatchError,
	traits::fungible::{InspectFreeze, Mutate},
};

type NativeBalance = <Test as crate::Config>::NativeBalance;

mod register {
	use super::*;

	#[test]
	fn register_with_sudo_success() {
		new_test_ext().execute_with(|| {
			// Go past genesis block so events get deposited.
			System::set_block_number(1);
			// Create Alice.
			let alice = 0;
			// Try to register alice with a sudo account.
			assert_ok!(Voting::register_voters(RuntimeOrigin::root(), alice));
			// Check that the event was generated.
			System::assert_last_event(Event::VoterRegistered { voter: alice }.into());
			// Check that the account was written to storage.
			assert!(<crate::pallet::RegisteredAccounts<Test>>::get(0).unwrap());
		});
	}

	#[test]
	fn register_without_sudo_fail() {
		new_test_ext().execute_with(|| {
			// Go past genesis block so events get deposited.
			System::set_block_number(1);
			// Create Alice and Bob.
			let alice = 0;
			let bob = 1;
			// Try to register a user. Should fail.
			assert_noop!(
				Voting::register_voters(RuntimeOrigin::signed(alice), bob),
				DispatchError::BadOrigin
			);
			// Check that the account was NOT written to storage.
			assert!(<crate::pallet::RegisteredAccounts<Test>>::get(0).is_none());
		});
	}

	#[test]
	fn already_registered() {
		new_test_ext().execute_with(|| {
			// Go past genesis block so events get deposited.
			System::set_block_number(1);
			// Register an account.
			let alice = 0;
			// Try to register alice with a sudo account. Should work.
			assert_ok!(Voting::register_voters(RuntimeOrigin::root(), alice));
			// Check that the event was generated.
			System::assert_last_event(Event::VoterRegistered { voter: alice }.into());
			// Check that the account was written to storage.
			assert!(<crate::pallet::RegisteredAccounts<Test>>::get(0).unwrap());
			// Try to register alice again. Should not work.
			assert_noop!(
				Voting::register_voters(RuntimeOrigin::root(), alice),
				Error::<Test>::VoterAlreadyRegistered
			);
			// Check there is only one account in storage.
			assert!(<crate::pallet::RegisteredAccounts<Test>>::get(1).is_none());
		});
	}
}

mod proposal {
	use super::*;

	#[test]
	fn add_proposal_from_unregistered_user_fails() {
		new_test_ext().execute_with(|| {
			// Go past genesis block so events get deposited.
			System::set_block_number(1);
			// Create an account.
			let alice = 0;
			// Give alice some tokens.
			assert_ok!(NativeBalance::mint_into(&alice, 100));
			// Alice is not registered so she can't make a proposal.
			assert_noop!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Error::<Test>::NotRegistered
			);
		});
	}

	#[test]
	fn add_proposal_from_registered_user() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			// Check that there is no proposal
			assert!(<crate::pallet::ProposalPool<Test>>::get(0).is_none());
			// Alice makes a proposal.
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]));
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());
			// Check that the proposal pool has been updated
			assert!(<crate::pallet::ProposalPool<Test>>::get(0).is_some());
			// Advance to the next block.
			System::set_block_number(2);
			// Bob makes a proposal.
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]));
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 1 }.into());
			// Check that the proposal pool has been updated
			assert!(<crate::pallet::ProposalPool<Test>>::get(1).is_some());
		});
	}
}

mod vote {
	use super::*;
	// add test when number of votes is 0.
	#[test]
	fn add_vote_from_unregistered_user_fails() {
		new_test_ext().execute_with(|| {
			let alice = 0;
			// No matter if the proposal exists, Alice is unregistered so that is the error she will
			// see.
			assert_noop!(
				Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0),
				Error::<Test>::NotRegistered
			);
		});
	}

	#[test]
	fn add_vote_proposal_doesnt_exist_fails() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			// Trying to vote for a proposal that doesnt exist.
			assert_noop!(
				Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0),
				Error::<Test>::ProposalDoesNotExist
			);
		});
	}

	#[test]
	fn add_vote_after_proposal_ends_fails() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Make a proposal and finish it.
			System::set_block_number(100000);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(alice), 0));
			// Trying to vote for a proposal that is finished.
			assert_noop!(
				Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0),
				Error::<Test>::VoteAlreadyEnded
			);
		});
	}

	#[test]
	fn add_vote_aye_increments() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();

			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));

			// Check storage was successfully set.
			let proposal = <crate::pallet::ProposalPool<Test>>::get(0).unwrap();
			assert_eq!(proposal.ayes, 1);

			// Cast 2 nays
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 2, true, 0));
			// Check storage was successfully set.
			let proposal = <crate::pallet::ProposalPool<Test>>::get(0).unwrap();
			assert_eq!(proposal.ayes, 3);
		});
	}

	#[test]
	fn add_vote_nay_increments() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();

			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 nay.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, false, 0));
			// Check the correct event is emitted.
			System::assert_last_event(Event::VoteAddedTo { proposal_id: 0, votes: 1 }.into());
			// Check storage was successfully set.
			let proposal = <crate::pallet::ProposalPool<Test>>::get(0).unwrap();
			assert_eq!(proposal.nays, 1);

			// Cast 2 nays.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 2, false, 0));
			// Check the correct event is emitted.
			System::assert_last_event(Event::VoteAddedTo { proposal_id: 0, votes: 2 }.into());
			// Check storage was successfully set.
			let proposal = <crate::pallet::ProposalPool<Test>>::get(0).unwrap();
			assert_eq!(proposal.nays, 3);
		});
	}

	#[test]
	fn voting_history_works() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited.
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());

			// Advance to the next block.
			System::set_block_number(2);
			// Bob makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]),
				Ok(())
			);
			// Assert that the correct event was deposited.
			System::assert_last_event(Event::ProposalCreated { proposal_id: 1 }.into());

			// Vote and check that the history is kept.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
		});
	}

	#[test]
	fn voting_history_multiple_works() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited.
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());

			// Advance to the next block.
			System::set_block_number(2);
			// Bob makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]),
				Ok(())
			);
			// Assert that the correct event was deposited.
			System::assert_last_event(Event::ProposalCreated { proposal_id: 1 }.into());

			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]),
				Ok(())
			);

			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			// Check voting history is added.
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			//Check the frozen amount is correct.
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);
			// Vote again on a different proposal.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 1), Ok(()));
			// Check voting history is added.
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 2);
			// Check frozen amount is increased.
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);
			// Vote on a third proposal.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 3, true, 2), Ok(()));
			// Check voting history is added.
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 3);
		});
	}

	#[test]
	fn freezing_tokens_works() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());
			// Check the frozen balance is zero before a vote.
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			// Check voting history is added.
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			// Check frozen_balance is increased.
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);
		});
	}

	#[test]
	fn freezing_tokens_many_new_votes_for_multiple_proposals_works() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());

			// Advance to the next block.
			System::set_block_number(2);
			// Bob makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 1 }.into());

			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(bob), vec![0, 1, 2, 3, 4]),
				Ok(())
			);

			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);
			//Check history is added and balance is frozen.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			//Check history is added and frozen balance is increased.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 1), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 2);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			// Checking frozen balance is not increased on this vote.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 3, true, 2), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 3);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);
		});
	}

	#[test]
	fn freezing_tokens_change_vote_one_proposal_works() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());

			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 3, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				9
			);
		});
	}

	#[test]
	fn freezing_tokens_change_vote_multiple_proposals_works() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 0 }.into());

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			// Assert that the correct event was deposited
			System::assert_last_event(Event::ProposalCreated { proposal_id: 1 }.into());

			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 1), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 2);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			// Frozen balance remains unchanged because of the frozen amount on proposal 1.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 3, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 2);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);
		});
	}
	#[test]
	fn too_many_votes_different_proposals_fails() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			for i in 0..100 {
				assert_ok!(Voting::make_proposal(
					RuntimeOrigin::signed(alice),
					vec![0, 1, 2, 3, 4]
				));
				// Cast 1 aye.
				assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, i));
			}
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);

			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Vote for the 101th proposal fails.
			assert_noop!(
				Voting::vote(RuntimeOrigin::signed(alice), 1, true, 100),
				Error::<Test>::TooManyVotes
			);
		});
	}

	#[test]
	fn cancel_vote_works() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();

			// Alice makes a proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);

			// Alice makes a second proposal.
			assert_eq!(
				Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3]),
				Ok(())
			);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 5, true, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 3, false, 0), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				9
			);

			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 6, false, 1), Ok(()));
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 2);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);

			// Frozen balance remains unchanged because of the frozen amount on proposal 1.
			assert_eq!(Voting::vote(RuntimeOrigin::signed(alice), 0, false, 0), Ok(()));
			// Voting history length shrinks because we have removed the vote
			assert_eq!(<crate::pallet::VotingHistory<Test>>::get(alice).unwrap().len(), 1);
			// Frozen amount remains unchanged, the one from proposal 1
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				36
			);
		});
	}
}

mod close_vote {
	use super::*;
	#[test]
	fn close_vote_aye_success() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));
			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			System::assert_last_event(Event::ProposalResultAye { proposal_id: 0 }.into());
			assert_eq!(<crate::pallet::ProposalPool<Test>>::get(0).unwrap().end, true);
		});
	}

	#[test]
	fn close_vote_nay_success() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 2, false, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));
			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			System::assert_last_event(Event::ProposalResultNay { proposal_id: 0 }.into());
			assert_eq!(<crate::pallet::ProposalPool<Test>>::get(0).unwrap().end, true);
		});
	}

	#[test]
	fn close_vote_tie_success() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, false, 0));
			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			System::assert_last_event(Event::ProposalResultTie { proposal_id: 0 }.into());
			assert_eq!(<crate::pallet::ProposalPool<Test>>::get(0).unwrap().end, true);
		});
	}

	#[test]
	fn close_vote_fail() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.

			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));
			System::set_block_number(3);
			assert_noop!(
				Voting::end_vote(RuntimeOrigin::signed(bob), 0),
				Error::<Test>::VotingPeriodNotOver
			);
		});
	}

	#[test]
	fn close_vote_already_closed_fail() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));
			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			System::assert_last_event(Event::ProposalResultAye { proposal_id: 0 }.into());
			assert_noop!(
				Voting::end_vote(RuntimeOrigin::signed(bob), 0),
				Error::<Test>::VoteAlreadyEnded
			);
		});
	}

	#[test]
	fn close_vote_proposal_doesnt_exist() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));
			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			System::assert_last_event(Event::ProposalResultAye { proposal_id: 0 }.into());
			assert_noop!(
				Voting::end_vote(RuntimeOrigin::signed(bob), 1),
				Error::<Test>::ProposalDoesNotExist
			);
		});
	}
}

mod claim_frozen_tokens {
	use super::*;

	#[test]
	fn voting_not_closed_fails() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			// Cast 1 aye.
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);

			assert_noop!(
				Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0),
				Error::<Test>::VotingPeriodNotOver
			);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
		});
	}

	#[test]
	fn proposal_doesnt_exist_fails() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();

			assert_noop!(
				Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0),
				Error::<Test>::ProposalDoesNotExist
			);
		});
	}

	#[test]
	fn no_votes_fails() {
		new_test_ext().execute_with(|| {
			let (alice, bob) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));

			assert_ok!(Voting::vote(RuntimeOrigin::signed(bob), 1, true, 0));

			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(bob), 0));
			assert_noop!(
				Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0),
				Error::<Test>::NoVotes
			);
		});
	}

	#[test]
	fn claim_smaller_than_max() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));

			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 5, false, 1));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(alice), 0));
			assert_ok!(Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0));
			System::assert_last_event(Event::NoTokensUnlocked.into());
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);
		});
	}

	#[test]
	fn claim_is_max() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));

			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 5, false, 1));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(alice), 1));
			assert_ok!(Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 1));
			System::assert_last_event(Event::TokensUnlocked.into());
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
		});
	}

	#[test]
	fn claim_thaws_last_proposal() {
		new_test_ext().execute_with(|| {
			let (alice, _) = test_utils::setup();
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));
			assert_ok!(Voting::make_proposal(RuntimeOrigin::signed(alice), vec![0, 1, 2, 3, 4]));

			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 1, true, 0));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
			assert_ok!(Voting::vote(RuntimeOrigin::signed(alice), 5, false, 1));
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				25
			);

			System::set_block_number(11);
			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(alice), 1));
			assert_ok!(Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 1));

			System::assert_last_event(Event::TokensUnlocked.into());
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);
			assert_noop!(
				Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0),
				Error::<Test>::VotingPeriodNotOver
			);

			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				1
			);

			assert_ok!(Voting::end_vote(RuntimeOrigin::signed(alice), 0));
			assert_ok!(Voting::claim_frozen_tokens(RuntimeOrigin::signed(alice), 0));
			System::assert_last_event(Event::TokensUnlocked.into());
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);
			assert_eq!(
				NativeBalance::balance_frozen(&crate::FreezeReason::AccountDeposit.into(), &alice),
				0
			);
		});
	}
}

mod test_utils {
	use super::*;

	pub fn setup() -> (u64, u64) {
		// Go past genesis block so events get deposited.
		System::set_block_number(1);
		// Register two accounts.
		let alice = 0;
		let bob = 1;
		// Give alice some tokens
		assert_ok!(NativeBalance::mint_into(&alice, 100));
		// give bob some tokens
		assert_ok!(NativeBalance::mint_into(&bob, 100));
		let _ = Voting::register_voters(RuntimeOrigin::root(), alice);
		let _ = Voting::register_voters(RuntimeOrigin::root(), bob);

		(alice, bob)
	}
}
