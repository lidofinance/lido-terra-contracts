import {
  Coins,
  getCodeId, getContractAddress,
  isTxError,
  LCDClient, MsgExecuteContract,
  MsgInstantiateContract, MsgMigrateContract,
  MsgStoreCode,
  StdFee,
  Wallet
} from "@terra-money/terra.js";
import * as fs from "fs";

export async function storeCode(terraClient: LCDClient, wallet: Wallet, contract_file: string): Promise<number> {
  console.log(`Storing ${contract_file}...`)

  const storeCode = new MsgStoreCode(
    wallet.key.accAddress,
    fs.readFileSync(contract_file).toString('base64')
  );
  const storeCodeTx = await wallet.createAndSignTx({
    msgs: [storeCode],
    fee: new StdFee(10000000, new Coins({uluna: 1000000}))
  });
  const storeCodeTxResult = await terraClient.tx.broadcast(storeCodeTx);

  if (isTxError(storeCodeTxResult)) {
    throw new Error(
      `store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
    );
  }

  let codeId = +getCodeId(storeCodeTxResult);
  console.log(`${contract_file} stored with code_id = ${codeId}`)

  return codeId
}

export async function instantiateContract(terraClient: LCDClient, wallet: Wallet, codeId: number, message: object, coins:Coins) {
  console.log(`Instantiating contract with code_id = ${codeId}...`)

  const instantiate = new MsgInstantiateContract(
    wallet.key.accAddress,
    wallet.key.accAddress,
    codeId,
    message,
    coins,
  );

  const instantiateTx = await wallet.createAndSignTx({
    msgs: [instantiate],
    fee: new StdFee(10000000, new Coins({uluna: 1000000}))
  });
  const instantiateTxResult = await terraClient.tx.broadcast(instantiateTx);

  if (isTxError(instantiateTxResult)) {
    throw new Error(
      `instantiate failed. code: ${instantiateTxResult.code}, codespace: ${instantiateTxResult.codespace}, raw_log: ${instantiateTxResult.raw_log}`
    );
  }

  return getContractAddress(instantiateTxResult);
}

export async function executeContract(terraClient: LCDClient, wallet: Wallet, contractAddress: string, message: object, coins: Coins) {
  const execute = new MsgExecuteContract(
    wallet.key.accAddress,
    contractAddress,
    message,
    coins
  );
  const executeTx = await wallet.createAndSignTx({
    msgs: [execute],
    fee: new StdFee(10000000, new Coins({uluna: 1000000}))
  });
  const executeTxResult = await terraClient.tx.broadcast(executeTx);
  if (isTxError(executeTxResult)) {
    throw new Error(
      `instantiate failed. code: ${executeTxResult.code}, codespace: ${executeTxResult.codespace}, raw_log: ${executeTxResult.raw_log}`
    );
  }
}

export async function migrateContract(terraClient: LCDClient, wallet: Wallet, contractAddress: string, newCodeId: number, message: object) {
  const migrate = new MsgMigrateContract(
    wallet.key.accAddress,
    contractAddress,
    newCodeId,
    message
  );
  const migrateTx = await wallet.createAndSignTx({
    msgs: [migrate],
    fee: new StdFee(10000000, new Coins({uluna: 1000000}))
  });
  const migrateTxResult = await terraClient.tx.broadcast(migrateTx);
  if (isTxError(migrateTxResult)) {
    throw new Error(
      `instantiate failed. code: ${migrateTxResult.code}, codespace: ${migrateTxResult.codespace}, raw_log: ${migrateTxResult.raw_log}`
    );
  }
}
