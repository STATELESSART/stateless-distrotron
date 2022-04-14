//! This contract distributes incoming payments of NEAR tokens to a list of recipient accounts.
//!
//! Methods: 
//!  Main money distro: pay_out
//!  Distro list management: set_recipients, get_receipients, add_recipient, remove_recipient
//!

use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::{env, ext_contract, near_bindgen, AccountId, Balance, Promise, PromiseResult};
use near_sdk::json_types::U128;
use near_sdk::serde_json::json;

near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(Default, BorshDeserialize, BorshSerialize)]
pub struct Distrotron {
}

// near_sdk::Balance is u128, so the JSON BigInt equiv is:
pub type JsonBalance = U128; 

const SOMEGAS: u64 = 10_000_000_000_000;
const LIST_MINTERS_GAS: u64 = SOMEGAS;

// the list_minters() API method on a Mintbase contract:
#[ext_contract(ext_mc)]
trait MinterContract {
    fn list_minters(&self) -> Vec<AccountId>;
}

#[ext_contract(ext_self)]
trait MyContract {
    fn list_minters_cb(&self) -> Promise;
    fn pay_out(&self, payees: Vec<AccountId>) -> Promise;
}


// our contract:
#[near_bindgen]
impl Distrotron {
    /// Returns the amount of NEAR that was paid to each recipient, in Yocto
    ///
    #[payable]
    pub fn pay_out(&mut self, payees: Vec<AccountId> ) -> Promise {
        self.__pay_out(payees)
    }

    #[payable]
    pub fn pay_out_net(&mut self, payees: Vec<AccountId> ) -> Promise {
        self.__pay_out_net(payees)
    }

    // abort if the payee list is in any way funny ...
    fn test_payees(&mut self, payees: Vec<AccountId> ) -> bool {
        // count the recipients.  
        // u32 only goes to 4 billion, and there are 8+ billion people in the world ...
        //let count: u64 = payees.len().try_into().unwrap(); 
        let count: u128 = payees.len().try_into().unwrap();
                
        // Fail if none.
        assert!(count > 0, "Empty recipient list");

        // count the money & fail if none.
        assert!(env::attached_deposit() > 0, "No payment attached");

        // Other tests:
        //
        // We'd like to parse the recipients list & make sure they're not garbled,
        // or else count on the transaction failing if it's not kosher.
        // But apparently we can't even test if accounts exist for some NEAR reason:
        // https://stackoverflow.com/questions/70819819/how-can-i-verify-if-a-near-address-is-valid-in-smart-contract/70820257#70820257
        
        // We could at least check that the IDs are valid format:
        // https://docs.rs/near-sdk/latest/near_sdk/env/fn.is_valid_account_id.html
        /*
        for acct_id in payees.clone().into_iter() {
            assert!( env::is_valid_account_id(acct_id.as_bytes()) ) ;
        }
        */
        // ... but what's the point if it can still fail?
        
        true
    }

    // Pay out the complete attached sum to the payees, no matter the gas.
    fn __pay_out(&mut self, payees: Vec<AccountId> ) -> Promise {
        self.test_payees(payees.clone());

        let total_payment: Balance = env::attached_deposit();
        let count: u128 = payees.len().try_into().unwrap();

        // Divide the yocto by the number of payees to get the individual payouts
        // NOTE: this is integer division;
        // the remainder, some integer yocto less than count, will be abandoned in this contract account.
        //
        // At time of writing, that sum is so much vastly less than one cent that i'm losing money just by thinking about it.
        //
        // But it occurs to me that this sort of leftover must exist everywhere in the universe of
        // traditional banking and blockchain.  One assumes, or hopes, that any sort of abuse or bug will be detected
        // by audits.
        
        let slice = total_payment / count; 
        let transfer_promise = self.transfer_to_each(payees, slice);
        let finish = Promise::new( env::current_account_id() ).function_call(b"report_payment".to_vec(), 
                                                                        json!({
                                                                            "amount": U128(slice)
                                                                        }).to_string().into_bytes(),
                                                                        0, // no payment 
                                                                        SOMEGAS
                                                                        );
        transfer_promise.then(finish)
    }


    // Pay out the sum to the payees, but hold back a small amount to cover gas.
    fn __pay_out_net(&mut self, payees: Vec<AccountId> ) -> Promise {

        self.test_payees(payees.clone());

        let total_payment: Balance = env::attached_deposit();
        let count: u128 = payees.len().try_into().unwrap();

        // estimate the gas costs:
        // 0.0001 near per Tgas seems to be the price lately.
        // that's 0.0001 near per 10^12 gas,
        // which is 0.0001 * 10^24 yoctonear per 10^12 gas,
        // so 0.0001 * 10^12 yocto per gas,
        // which is 1 * 10^9 yocto.
        let ypg = 1000000000;

        // How to truly know the gas price tho?  There's a cross-contract call you can make to see it on some other
        // recent block ...

        // 1 Tgas = 10^12 gas, docs suggest it costs .45 Tgas to send funds, so we can
        //   calculate that
        let est_gas_per_payee:u128 = 45000000000; // 0.45 Tgas (4.5^11)
        // convert to yocto
        let est_fee_per_payee:Balance = est_gas_per_payee * ypg;
        
        //   We can also do testing to get a pretty good idea of the gas cost of pay_out,
        //   and see how it expands / contracts with the distro list.
        //   Then when running I think we can maybe get some idea of the current gas cost,
        //   and estimate something that way.
        let est_gas_other:u128 = 100000000000; // 0.1 Tgas, for now.
        // convert to near
        let est_fee_other:Balance = est_gas_other * ypg;

        // and we also have to account for the final function call:
        //let est_gas_end:u128 = 100000000000; // 0.1 Tgas, for now.
        // Nope, turns out to be more ...
        let est_gas_end:u128 = 1000000000000; // 0.1 Tgas, for now.
        let est_fee_end:Balance = est_gas_end * ypg;

        //   Question is, can that gas cost change during the running of this method?  I think not
        //   if it's not a cross-contract call.  I think it's all in the current block at the
        //   current price ... we'll see.
        
        // subtract the gas costs from the yocto:
        let net_payment = ( (total_payment - est_fee_other) - (count * est_fee_per_payee) ) - est_fee_end ;
        
        // divide the remaining yocto by the number of payees to get the individual payouts
        let net_slice = net_payment / count; 

        // return the rest to the caller, to cover the gas cost
        let refund = total_payment - (net_slice * count);

        let transfer_promise = self.transfer_to_each(payees, net_slice);

        // don't forget your change!
        let nickelback = Promise::new(env::signer_account_id()).transfer(refund);

        let finish = Promise::new( env::current_account_id() ).function_call(b"report_payment".to_vec(), 
                                                                        json!({
                                                                            //"amount": JsonBalance(net_slice)  // nope.
                                                                            "amount": U128(net_slice)
                                                                        }).to_string().into_bytes(),
                                                                        0, // no payment 
                                                                        est_gas_end.try_into().unwrap()
                                                                        );
        transfer_promise.then(nickelback).then(finish)
    }

    fn transfer_to_each(&self, payees: Vec<AccountId>, sum: Balance) -> Promise {
        // pay each payee in a loop
        let promises: Vec<Promise> = payees.into_iter().map(|p| {
            Promise::new(p.to_string()).transfer(sum)
        } ).collect();
        
        // boil all those promises down into a super-promise
        let mut big_p = promises[0].clone();
        for pi in 1..promises.len() {
            big_p = big_p.and(promises[pi].clone());
        }

        big_p
    }

    pub fn report_payment(&self, amount: JsonBalance) -> JsonBalance { 
        // Return the count of how much each payee received:
        amount
    }

    // Given a contract ID, get the list of minters from that contract's list_minters() method,
    // then distribute the attached funds to that list of minters via pay_out().
    #[payable]
    pub fn pay_minters(&mut self, minter_contract: AccountId) -> Promise {
        assert!( env::is_valid_account_id(minter_contract.as_bytes()), "Invalid contract ID" ) ;
        ext_mc::list_minters(&minter_contract, 0, LIST_MINTERS_GAS)
            .then(ext_self::list_minters_cb(&env::current_account_id(), 
                    env::attached_deposit(), 
                    ////////////////////////////////
                    // complicated gas accounting:
                    // send along all the gas we got, 
                    env::prepaid_gas()  
                    // except for:
                    - (
                    // what we've used so far,
                    env::used_gas() 
                    // plus what we just attached to the other promise above,
                     + LIST_MINTERS_GAS
                    // plus a wee bit more, just so this last command itself can excecute
                     + SOMEGAS 
                    ) 
                    ////////////////////////////////
                  ))
    }

    #[payable]
    pub fn list_minters_cb(&mut self) -> Promise {
        // pattern from https://docs.near.org/docs/tutorials/contracts/xcc-rust-cheatsheet :
        assert_eq!(env::promise_results_count(), 1, "This is a callback method");
        // what else can I secure here?  can I check that the caller is the signer?  does that help?
        // can i make sure the caller is the contract owner? does that help?

        match env::promise_result(0) {
            PromiseResult::NotReady => unreachable!(),
            PromiseResult::Failed => env::panic(b"minter contract failure"),
            PromiseResult::Successful(val) => {
                let payees = near_sdk::serde_json::from_slice::<Vec<AccountId>>(&val).unwrap();

               // test for length:
                assert!( payees.len() > 0, "no minters found");
                self.__pay_out(payees)
            }
        }
    }



    pub fn be_good(&self) -> bool {
        true
    }
    
}




// use the attribute below for unit tests
#[cfg(test)]
mod unit_tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    // part of writing unit tests is setting up a mock context
    // in this example, this is only needed for env::log in the contract
    // this is also a useful list to peek at when wondering what's available in env::*
    fn get_context(input: Vec<u8>, is_view: bool) -> VMContext {
        VMContext {
            current_account_id: "alice.testnet".to_string(),
            signer_account_id: "robert.testnet".to_string(),
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: "jane.testnet".to_string(),
            input,
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    fn to_ynear(near: Balance) -> Balance {
        near * 10u128.pow(24)
    }

    fn bob() -> AccountId {
        "bob.testnet".to_string()
    }

    fn carol() -> AccountId {
        "carol.testnet".to_string()
    }

    fn dick() -> AccountId {
        "dick.testnet".to_string()
    }

    fn eve() -> AccountId {
        "eve.testnet".to_string()
    }

    // fn frank() -> AccountId {
    //     "frank.testnet".to_string()
    // }
    //
    // fn grace() -> AccountId {
    //     "grace.testnet".to_string()
    // }


    #[test]
    // pay_out should fail with no list of recipients:
    #[should_panic(
        expected = r#"Empty recipient list"#
    )]
    fn pay_out_1() { 
        let mut c = get_context(vec![], false);
        c.attached_deposit = to_ynear(10);
        testing_env!(c);
        let mut contract = Distrotron {};

        let chumps = vec![];
        let _cut = contract.pay_out(chumps);
    }


    #[test]
    // pay_out should fail if no payment attached
    #[should_panic(
        expected = r#"No payment attached"#
    )]
    fn pay_out_2() { 
        let c = get_context(vec![], false);
        testing_env!(c);
        let mut contract = Distrotron {};
        let chumps = vec![bob(), carol(), dick(), eve()];
        let _cut = contract.pay_out(chumps);
    }

    //#[test]
    // // pay_out should succeed with multiple recipients
    // fn pay_out_4() { 
    //     let c = get_context(vec![], false);
    //     c.attached_deposit = to_ynear(10);
    //     testing_env!(c);
    //     let mut contract = Distrotron {};
    //
    //     let chumps = vec![bob(), carol(), dick(), eve()];
    //
    //     // how much money does bob have before the call?  probably not much?
    //     let before = chumps[0].account().unwrap().amount;
    //     let cut = contract.pay_out(chumps);
    //     let after = chumps[0].account().unwrap().amount;
    //     assert_eq!(after - before, cut, "bob was ripped off!");
    // }

    // more functional tests are performed in the Simulator & Sandbox.

    // pay_minters() should fail if argument is invalid
    #[test]
    #[should_panic(
        expected = r#"Invalid contract ID"#
    )]
    fn pay_minters_1() {
        let mut c = get_context(vec![], false);
        c.attached_deposit = to_ynear(10);
        testing_env!(c);
        let mut contract = Distrotron {};
        contract.pay_minters("i".to_string()); // invalid; minimum length is 2
    }
    // // should fail if list_minters fails on the target contact
    // #[test]
    // #[should_panic(
    //     expected = r#"minter contract failure"#
    // )]
    // fn pay_minters_2() {
    //     let c = get_context(vec![], false);
    //     c.attached_deposit = to_ynear(10);
    //     testing_env!(c);
    //     let mut contract = Distrotron {};
    // }
    // // should fail if list_minters returns no minters
    // #[test]
    // #[should_panic(
    //     expected = r#"no minters found"#
    // )]
    // fn pay_minters_3() {
    //     let c = get_context(vec![], false);
    //     c.attached_deposit = to_ynear(10);
    //     testing_env!(c);
    //     let mut contract = Distrotron {};
    // }
    // // should punt to pay_out otherwise.
    // #[test]
    // fn pay_minters_4() {
    //     let c = get_context(vec![], false);
    //     c.attached_deposit = to_ynear(10);
    //     testing_env!(c);
    //     let mut contract = Distrotron {};
    // }

}
