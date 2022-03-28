const nearAPI = require("near-api-js");
const BN = require("bn.js");
const fs = require("fs").promises;
const assert = require("assert").strict;
const path = require("path");
const { utils } = nearAPI;


function getConfig(env) {
  switch (env) {
    case "testnet":
      return {
        networkId: "testnet",
        nodeUrl: "http://23.23.23.101:3030",
        masterAccount: "mykletest.testnet",
        contractAccount: "distro_test",
				stubAccount: "stub",
				keyPath: "/Volumes/External/mykle/.near-credentials/testnet/mykletest.testnet.json",
      };
    case "sandbox":
      return {
        networkId: "sandbox",
        nodeUrl: "http://23.23.23.101:3030",
        masterAccount: "test.near",
        contractAccount: "distro",
				stubAccount: "stub",
				keyPath: "/Volumes/External/mykle/tmp/near-sandbox/validator_key.json",
      };
    case "local":
      return {
        networkId: "sandbox",
        nodeUrl: "http://localhost:3030",
        masterAccount: "test.near",
        contractAccount: "distro",
        stubAccount: "stub",
        keyPath: "/tmp/near-sandbox/validator_key.json",
      };
  }
}

function n2y(near) {
	return utils.format.parseNearAmount(near.toString());
}
function y2n(yocto) { 
	return utils.format.formatNearAmount(yocto);
}

function fullAccountName(prefix){
	return prefix + '.' + config.masterAccount;
}

// const contractMethods = {
//   changeMethods: ["pay_minters", "pay_out"],
// };

let config;
let masterAccount;
let masterKey;
let pubKey;
let keyStore;
let near;

async function initNear() {
  config = getConfig(process.env.NEAR_ENV || "sandbox");
  const keyFile = require(config.keyPath);
  masterKey = nearAPI.utils.KeyPair.fromString(
    keyFile.secret_key || keyFile.private_key
  );
  pubKey = masterKey.getPublicKey();
  keyStore = new nearAPI.keyStores.InMemoryKeyStore();
  keyStore.setKey(config.networkId, config.masterAccount, masterKey);
  near = await nearAPI.connect({
    deps: {
      keyStore,
    },
    networkId: config.networkId,
    nodeUrl: config.nodeUrl,
  });
  masterAccount = new nearAPI.Account(near.connection, config.masterAccount);
  console.log("Finish init NEAR");
}

async function createTestUser(
  accountPrefix, initBalance = 100
) {
	//let accountId = accountPrefix + "." + config.masterAccount;
  let accountId = fullAccountName(accountPrefix);
  keyStore.setKey(config.networkId, accountId, masterKey);
  const account = new nearAPI.Account(near.connection, accountId);

	// delete it first, 
	try { 
		await account.deleteAccount(config.masterAccount);
	} catch {
		// fails if it didn't exist
	}

	// then recreate it.
  await masterAccount.createAccount(
    accountId,
    pubKey,
		n2y(initBalance)
  );
	console.log("created " + accountId);
	return account;
}

async function initContracts() {
  const contractwasm = await fs.readFile(path.resolve(__dirname, "../../target/wasm32-unknown-unknown/release/distrotron.wasm"));
	const _contractAccount = await createTestUser(config.contractAccount, 10000);
	await _contractAccount.deployContract(contractwasm);

	const stubwasm = await fs.readFile(path.resolve(__dirname, "../../target/wasm32-unknown-unknown/release/stub.wasm"));
	const _stubAccount = await createTestUser(config.stubAccount);
	await _stubAccount.deployContract(stubwasm);

  console.log("contracts deployed, test accounts created");
	return _contractAccount;
}

async function totalBalance(acct){
	b = await acct.getAccountBalance();
	return b.total;
}

async function test_pay_out() {
  // 1. Creates testing accounts and deploys a contract
  await initNear();
	await initContracts();

  const alice = await createTestUser(
    "alice",
  );

  const bob = await createTestUser(
    "bob",
  );

	const carol = await createTestUser("carol");

	let balances = {
		before: {
			"alice": y2n(await totalBalance(alice)),
			"bob": y2n(await totalBalance(bob)),
			"carol": y2n(await totalBalance(carol)),
		}
	};

	const distro = new nearAPI.Contract(
		carol, // will call it
		fullAccountName(config.contractAccount), // name (string) of acct where contract is deployed
		{changeMethods: ["pay_out"]}
	);

	// 2. have carol send some money to distrotron
	
	net_payment = await distro.pay_out( { 
		args: {
			payees: [alice.accountId, bob.accountId]
	}, 
		gas: "300000000000000", // attached GAS (optional)
		amount: n2y(10),					// attached near
	});

	// 3. check that it was distributed to alice and bob
	
	balances.after = {
			"alice": y2n(await totalBalance(alice)),
			"bob": y2n(await totalBalance(bob)),
			"carol": y2n(await totalBalance(carol)),
	};

	console.log("Net payment: " + y2n(net_payment) + " NEAR");
	console.log(balances);
}  

async function test(){
	const started = new Date();
	await test_pay_out();
	const finished = new Date();
	console.log(`execution time: ${(finished - started)/1000} seconds`);
}

test();
