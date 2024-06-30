use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{trace, warn};

use crate::{
	blockchain_service::{BlockchainService, ContractEvent},
	bridge_contracts::{BridgeContractCounterparty, BridgeContractInitiator},
	bridge_monitoring::{BridgeContractCounterpartyEvent, BridgeContractInitiatorEvent},
	bridge_service::{
		active_swap::ActiveSwapEvent,
		events::{CEvent, CWarn, IEvent, IWarn},
	},
	types::Convert,
};

pub mod active_swap;
pub mod events;

use self::{active_swap::ActiveSwapMap, events::Event};

pub struct BridgeService<B1, B2>
where
	B1: BlockchainService,
	B2: BlockchainService,
{
	pub blockchain_1: B1,
	pub blockchain_2: B2,

	pub active_swaps_b1_to_b2: ActiveSwapMap<B1, B2>,
	pub active_swaps_b2_to_b1: ActiveSwapMap<B2, B1>,
}

impl<B1, B2> BridgeService<B1, B2>
where
	B1: BlockchainService + 'static,
	B2: BlockchainService + 'static,
{
	pub fn new(blockchain_1: B1, blockchain_2: B2) -> Self {
		Self {
			active_swaps_b1_to_b2: ActiveSwapMap::build(
				blockchain_1.initiator_contract().clone(),
				blockchain_2.counterparty_contract().clone(),
			),
			active_swaps_b2_to_b1: ActiveSwapMap::build(
				blockchain_2.initiator_contract().clone(),
				blockchain_1.counterparty_contract().clone(),
			),
			blockchain_1,
			blockchain_2,
		}
	}
}

impl<B1, B2> Stream for BridgeService<B1, B2>
where
	B1: BlockchainService + 'static,
	B2: BlockchainService + 'static,

	<B1::InitiatorContract as BridgeContractInitiator>::Hash: From<B2::Hash>,
	<B1::InitiatorContract as BridgeContractInitiator>::Address: From<B2::Address>,

	<B1::CounterpartyContract as BridgeContractCounterparty>::Hash: From<B2::Hash>,
	<B1::CounterpartyContract as BridgeContractCounterparty>::Address: From<B2::Address>,

	<B2::InitiatorContract as BridgeContractInitiator>::Hash: From<B1::Hash>,
	<B2::InitiatorContract as BridgeContractInitiator>::Address: From<B1::Address>,

	<B2::CounterpartyContract as BridgeContractCounterparty>::Hash: From<B1::Hash>,
	<B2::CounterpartyContract as BridgeContractCounterparty>::Address: From<B1::Address>,

	<B1 as BlockchainService>::Hash: Convert<B2::Hash>,
	<B2 as BlockchainService>::Hash: Convert<B1::Hash>,

	<B1 as BlockchainService>::Hash: From<<B2 as BlockchainService>::Hash>,
	<<B1 as BlockchainService>::InitiatorContract as BridgeContractInitiator>::Hash:
		From<<B2 as BlockchainService>::Hash>,
{
	type Item = Event<B1, B2>;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = self.get_mut();

		use ActiveSwapEvent::*;

		// Handle active swaps initiated from blockchain 1
		match this.active_swaps_b1_to_b2.poll_next_unpin(cx) {
			Poll::Ready(Some(event)) => {
				trace!("BridgeService: Received event from active swaps B1 -> B2: {:?}", event);
				match event {
					BridgeAssetsLocked(bridge_transfer_id) => {
						trace!(
							"BridgeService: Bridge assets locked for transfer {:?}",
							bridge_transfer_id
						);
						// The smart contract has been called on blockchain_2. Now, we have to wait for
						// confirmation from the blockchain_2 event.
					}
					BridgeAssetsLockingError(error) => {
						warn!("BridgeService: Error locking bridge assets: {:?}", error);
						// An error occurred while calling the lock_bridge_transfer_assets method. This
						// could be due to a network error or an issue with the smart contract call.

						// This will cause the call to be retried a number of tries before giving up
					}
					BridgeAssetsCompleted(bridge_transfer_id) => {
						trace!(
							"BridgeService: Bridge assets completed for transfer {:?}",
							bridge_transfer_id
						);
						// The bridge assets have been successfully completed.
					}
					BridgeAssetsCompletingError(error) => {
						warn!("BridgeService: Error completing bridge assets: {:?}", error);
						// An error occurred while called the complete_bridge_transfer method. This could
						// be due to a network error or an issue with the smart contract call.

						// This will cause the call to be retried a number of tries before giving up
					}
				}
			}
			Poll::Ready(None) => {
				trace!("BridgeService: Active swaps B1 -> B2 has no more events");
			}
			Poll::Pending => {
				trace!("BridgeService: Active swaps B1 -> B2 has no events at this time");
			}
		}

		// Handle active swaps initiated from blockchain 2
		match this.active_swaps_b2_to_b1.poll_next_unpin(cx) {
			Poll::Ready(Some(event)) => {
				trace!("BridgeService: Received event from active swaps B2 -> B1: {:?}", event);
				match event {
					BridgeAssetsLocked(bridge_transfer_id) => {
						trace!(
							"BridgeService: Bridge assets locked for transfer {:?}",
							bridge_transfer_id
						);
					}
					BridgeAssetsLockingError(error) => {
						warn!("BridgeService: Error locking bridge assets: {:?}", error);
					}
					BridgeAssetsCompleted(_) => todo!(),
					BridgeAssetsCompletingError(_) => todo!(),
				}
			}
			Poll::Ready(None) => {
				trace!("BridgeService: Active swaps B2 -> B1 has no more events");
			}
			Poll::Pending => {
				trace!("BridgeService: Active swaps B2 -> B1 has no events at this time");
			}
		}

		use Event::*;

		match this.blockchain_1.poll_next_unpin(cx) {
			Poll::Ready(Some(event)) => {
				trace!("BridgeService: Received event from blockchain service 1: {:?}", event);
				match event {
					ContractEvent::InitiatorEvent(initiator_event) => {
						trace!("BridgeService: Initiator event from blockchain service 1");
						match initiator_event {
							BridgeContractInitiatorEvent::Initiated(ref details) => {
								// Bridge transfer initiated. Now, as the counterparty, we should lock
								// the appropriate tokens using the same secret.
								if this
									.active_swaps_b1_to_b2
									.already_executing(&details.bridge_transfer_id)
								{
									warn!("BridgeService: Bridge transfer {:?} already present, monitoring should only return event once", details.bridge_transfer_id);
									return Poll::Ready(Some(B1I(IEvent::Warn(
										IWarn::AlreadyPresent(details.clone()),
									))));
								}

								this.active_swaps_b1_to_b2.start_bridge_transfer(details.clone());
								return Poll::Ready(Some(B1I(IEvent::ContractEvent(
									initiator_event,
								))));
							}
							BridgeContractInitiatorEvent::Completed(_) => {
								// The BridgeService successfully completed the swap on blockchain 1 after
								// obtaining the pre-image of the hash, as the initiator claimed the funds.
								return Poll::Ready(Some(B1I(IEvent::ContractEvent(
									initiator_event,
								))));
							}
							BridgeContractInitiatorEvent::Refunded(_) => todo!(),
						}
					}
					ContractEvent::CounterpartyEvent(_) => {
						trace!("BridgeService: Counterparty event from blockchain service 1");
					}
				}
			}
			Poll::Ready(None) => {
				trace!("BridgeService: Blockchain service 1 has no more events");
			}
			Poll::Pending => {
				trace!("BridgeService: Blockchain service 1 has no events at this time");
			}
		}

		match this.blockchain_2.poll_next_unpin(cx) {
			Poll::Ready(Some(event)) => {
				trace!("BridgeService: Received event from blockchain service 2: {:?}", event);
				match event {
					ContractEvent::InitiatorEvent(_) => {
						trace!("BridgeService: Initiator event from blockchain service 2");
					}
					ContractEvent::CounterpartyEvent(event) => {
						trace!("BridgeService: Counterparty event from blockchain service 2");
						use BridgeContractCounterpartyEvent::*;
						match event {
							Locked(ref _details) => {
								// Asset locking on the counterpart bridge has been successfully confirmed. The
								// system will now begin monitoring for the claim event, allowing the bridge to
								// access the secret and unlock the corresponding funds on the opposite end.
								return Poll::Ready(Some(B2C(CEvent::ContractEvent(event))));
							}
							Completed(ref details) => {
								// The client implementation has successfully unlocked the assets on the
								// counterparty bridge. Consequently, the bridge will now proceed to claim the
								// funds on the initiator's side using the provided pre-image

								match this
									.active_swaps_b1_to_b2
									.complete_bridge_transfer(details.clone())
								{
									Ok(_) => {
										trace!(
											"BridgeService: Bridge transfer completed successfully"
										);
										return Poll::Ready(Some(B2C(CEvent::ContractEvent(
											event,
										))));
									}
									Err(error) => {
										warn!(
											"BridgeService: Error completing bridge transfer: {:?}",
											error
										);
										// This situation is critical and requires immediate attention. The bridge has
										// received an event from the blockchain to close the active swap but failed to
										// do so, potentially resulting in fund loss (for the bridge operator). To address this issue, we should
										// make a manual call to the contract using the available details.
										match error {
											active_swap::ActiveSwapMapError::NonExistingSwap => {
												return Poll::Ready(Some(B2C(CEvent::Warn(
													CWarn::CannotCompleteUnexistingSwap(
														details.clone(),
													),
												))));
											}
										}
									}
								}
							}
						}
					}
				}
			}
			Poll::Ready(None) => {
				trace!("BridgeService: Blockchain service 2 has no more events");
			}
			Poll::Pending => {
				trace!("BridgeService: Blockchain service 2 has no events at this time");
			}
		}

		Poll::Pending
	}
}
