import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { DriftVault } from '../target/types/drift_vault';

import { Pyth } from '../target/types/pyth';
import PythIDL from '../target/idl/pyth.json';

import { ClearingHouse } from '../deps/protocol-v1/target/types/clearing_house';
import ClearingHouseIDL from '../deps/protocol-v1/target/idl/clearing_house.json';

import * as token from "@solana/spl-token";
import * as web3 from "@solana/web3.js";

import { assert } from 'chai';
import { Keypair, PublicKey } from '@solana/web3.js';
import { BN } from '@project-serum/anchor';

import { mockUSDCMint, mockUserUSDCAccount, mockOracle } from './testHelpers';

import * as drift from '../deps/protocol-v1/sdk/src';
import { Admin } from '../deps/protocol-v1/sdk/src';
import {
	MARK_PRICE_PRECISION,
	PositionDirection,
	ZERO,
} from '../deps/protocol-v1/sdk';

import { getFeedData, setFeedPrice } from "../ts/mockPythUtils";

import * as fs from "fs";

const clearingHousePublicKey = new PublicKey(
  'AsW7LnXB9UA1uec9wi9MctYTgTz7YH9snhxd16GsFaGX'
);
const pythPublicKey = new PublicKey(
  '6bgJrRngVsFzCFkjd5PkVKMtb1C3JXgYnEFLhkJPtnEp'
);

describe('drift_vault', () => {

  let provider = anchor.Provider.env(); 
  let connection = provider.connection;
  anchor.setProvider(provider);

  // @ts-ignore
  const vault_program = anchor.workspace.DriftVault as Program<DriftVault>;
  let CH_program: Program<ClearingHouse>;   
  let pyth_program: Program<Pyth>;

  // CH setup 
  let usdcMint: Keypair;
	let userUSDCAccount: Keypair;
	const clearingHouse = Admin.from(
		provider.connection,
		provider.wallet,
		clearingHousePublicKey
	);

  const usdcAmount = new BN(100_000 * 10 ** 6);

  let sqrtk = 1e8;
  const PAIR_AMT = sqrtk;
	const ammInitialQuoteAssetAmount = new BN(PAIR_AMT).mul(MARK_PRICE_PRECISION);
	const ammInitialBaseAssetAmount = new BN(PAIR_AMT).mul(MARK_PRICE_PRECISION);

	const marketIndex = new BN(0);

  // oracle price: base/quote amm equal thus price = 1 to match 
  let usdcSolPrice = 1; 

  // setup programs + fake market 
  before(async () => {
    
    // setup other programs
    CH_program = new anchor.Program(
      ClearingHouseIDL as anchor.Idl, 
      clearingHousePublicKey, 
      provider
    ) as Program<ClearingHouse>;

    pyth_program = new anchor.Program(
      PythIDL as anchor.Idl, 
      pythPublicKey, 
      provider
    ) as Program<Pyth>;

		usdcMint = await mockUSDCMint(provider);
		userUSDCAccount = await mockUserUSDCAccount(usdcMint, usdcAmount, provider);
		await clearingHouse.initialize(usdcMint.publicKey, true);
		await clearingHouse.subscribeToAll();

		const solUsd = await mockOracle(usdcSolPrice, -6);
    // const periodicity = new BN(60 * 60); // 1 HOUR
		const periodicity = new BN(1); // 1 SECOND

    // fund the market collateral to pay for funding rates  
    let clearingHouseState = clearingHouse.getStateAccount();
    const fund_amm_ix = await token.Token.createMintToInstruction(
      token.TOKEN_PROGRAM_ID,
      usdcMint.publicKey,
      clearingHouseState.collateralVault,
      provider.wallet.publicKey,
      [],
      100_000 * 10 ** 6 
    );
    await provider.send(new web3.Transaction().add(fund_amm_ix))

		await clearingHouse.initializeMarket(
			marketIndex,
			solUsd,
			ammInitialBaseAssetAmount,
			ammInitialQuoteAssetAmount,
			periodicity, 
      drift.PEG_PRECISION
		);
	});

  after(async () => {
		await clearingHouse.unsubscribe();
	});

  let vault_mint, vault_mint_b;
  let vault_state, vault_state_b;
  let vault_collateral, vault_collateral_b;
  let authority, authority_b;
  let user_positions, user_positions_b;
  let user_account, user_account_b;
  let clearingHouseStatePk;
  let clearingHouseState;

  it('initializes the vault', async () => {
    // derive pool mint PDA 
    [vault_mint, vault_mint_b] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("vault_mint")], 
      vault_program.programId,
    );
    [vault_state, vault_state_b] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("vault_state")], 
      vault_program.programId,
    );
    [vault_collateral, vault_collateral_b] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("vault_collateral")], 
      vault_program.programId,
    );
    [authority, authority_b] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("authority")], 
      vault_program.programId,
    );
    [user_positions, user_positions_b] = await web3.PublicKey.findProgramAddress(
      [Buffer.from("user_positions")], 
      vault_program.programId,
    );

    // account for authority 
    [user_account, user_account_b] = await drift.getUserAccountPublicKeyAndNonce(
      CH_program.programId,
      authority,
    );
    clearingHouseStatePk = await clearingHouse.getStatePublicKey(); 
    clearingHouseState = clearingHouse.getStateAccount();

    await vault_program.rpc.initializeVault(
      user_account_b, 
      authority_b,
      user_positions_b,
      {
        accounts: {
          payer: provider.wallet.publicKey, 

          authority: authority, 
          clearingHouseState: clearingHouseStatePk,
          clearingHouseUser: user_account,
          clearingHouseUserPositions: user_positions,

          vaultMint: vault_mint, 
          vaultState: vault_state, 

          vaultCollateral: vault_collateral,
          collateralMint: usdcMint.publicKey, 
          
          clearingHouseProgram: CH_program.programId,
          systemProgram: web3.SystemProgram.programId,
          rent: web3.SYSVAR_RENT_PUBKEY,
          tokenProgram: token.TOKEN_PROGRAM_ID,
        }, 
      }
    )
  });

  async function get_token_balance(token_ata: web3.PublicKey) {
    var vault_balance = await connection.getTokenAccountBalance(token_ata);
    return new BN(vault_balance.value.amount)
  }

  let user_vault_ata; 
  it('deposits into vault', async () => {

    const _depositAmount = 1_000; 
    const depositAmount = new BN(_depositAmount * 10 ** 6);

    // create ata of vault mint 
    user_vault_ata = await token.Token.getAssociatedTokenAddress(
      token.ASSOCIATED_TOKEN_PROGRAM_ID, 
      token.TOKEN_PROGRAM_ID, 
      vault_mint, 
      provider.wallet.publicKey, 
    );

    let ata_ix = await token.Token.createAssociatedTokenAccountInstruction(
      token.ASSOCIATED_TOKEN_PROGRAM_ID, 
      token.TOKEN_PROGRAM_ID, 
      vault_mint, 
      user_vault_ata, 
      provider.wallet.publicKey,
      provider.wallet.publicKey,
    );

    // deposit USDC in there lfg
    let deposit_ix = await vault_program.instruction.deposit(
      depositAmount,
      authority_b,
      {
        accounts: {
          owner: provider.wallet.publicKey, 
          userVaultAta: user_vault_ata, 
          userCollateralAta: userUSDCAccount.publicKey,
          vaultCollateralAta: vault_collateral,

          vaultMint: vault_mint, 
          vaultState: vault_state, 
          
          authority: authority, 
          clearingHouseUserPositions: user_positions,
          clearingHouseUser: user_account,
          
          clearingHouseState: clearingHouseStatePk,
          clearingHouseCollateralVault: clearingHouseState.collateralVault,
          clearingHouseMarkets: clearingHouseState.markets,
          clearingHouseFundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
          clearingHouseDepositHistory: clearingHouseState.depositHistory,

          clearingHouseProgram: CH_program.programId,
          tokenProgram: token.TOKEN_PROGRAM_ID,
        }
      }
    )

    let tx = new web3.Transaction().add(...[ata_ix, deposit_ix])

    var user_colleteral_balance_start = await get_token_balance(userUSDCAccount.publicKey);
    let userAccount_start = await CH_program.account.user.fetch(user_account);

    await provider.send(tx);

    // more vault mints 
    var user_vault_balance = await get_token_balance(user_vault_ata)
    assert(user_vault_balance.gt(drift.ZERO))

    // user less USDC 
    var user_colleteral_balance_end = await get_token_balance(userUSDCAccount.publicKey);
    assert(user_colleteral_balance_start.gt(user_colleteral_balance_end))

    // vault more USDC
    let userAccount = await CH_program.account.user.fetch(user_account);
    assert(userAccount.collateral.eq(depositAmount))    
    assert(userAccount_start.collateral.lt(userAccount.collateral))    
  })

  async function view_market_state() {
    const pythClient = new drift.PythClient(connection)
    const market = clearingHouse.getMarket(marketIndex); 
    const solUsd = market.amm.oracle;

    // current mark + oracle price 
    var solUsdcData = await getFeedData(pyth_program, solUsd)
    var currentMarketPrice = drift.calculateMarkPrice(market);
    console.log("sol usdc price (mark):", currentMarketPrice.toString()) 
    console.log("sol usdc price (oracle):", solUsdcData.price) 
    
    // funding rate
    var estimated_funding = await drift.calculateEstimatedFundingRate(
      market, 
      await pythClient.getOraclePriceData(solUsd),
      new BN(1), 
      "interpolated"
    );
    console.log("estimated funding:", estimated_funding.toString());
  }

  async function update_twaps(oracle_increase, mark_increase) {
    const market = clearingHouse.getMarket(marketIndex); 
    const solUsd = market.amm.oracle;
    
    // update oracle  
    var solUsdcData = await getFeedData(pyth_program, solUsd)
    let new_oracle_price = solUsdcData.price * oracle_increase; 
    await setFeedPrice(pyth_program, new_oracle_price, solUsd)

    var currentMarketPrice = drift.calculateMarkPrice(market);
    let new_mark_price = currentMarketPrice.mul(new BN(mark_increase));
    
    // hacky hacky hack hack 
    await CH_program.rpc.updateTwaps(
      marketIndex, 
      new_mark_price,
      new BN(new_oracle_price * 10 ** 10), // mark percision? 
      {
        accounts: {
          state: clearingHouseStatePk, 
          markets: clearingHouseState.markets,
          oracle: solUsd, 
          fundingRateHistory: clearingHouseState.fundingRateHistory
        }
      }
    )
  }

  it("opens a long when mark < oracle", async () => {
    const market = clearingHouse.getMarket(marketIndex); 
    const solUsd = market.amm.oracle;

    // oracle moves up => oracle > mark => shorts pay longs
    await update_twaps(1.02, 1); 

    // view_market_state()

    let ix = vault_program.instruction.updatePosition(
      marketIndex,
      authority_b,
      {
        accounts: {
          authority: authority, 
          userPositions: user_positions,
          
          state: clearingHouseStatePk,
          user: user_account,
          markets: clearingHouseState.markets,
          tradeHistory: clearingHouseState.tradeHistory,
          fundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
          fundingRateHistory: clearingHouseState.fundingRateHistory,
          oracle: solUsd,
          clearingHouseProgram: CH_program.programId,
        }
      }
    )
    let tx = new web3.Transaction().add(ix);
    
    // let resp = await provider.simulate(tx)
    // console.log(resp)

    await provider.send(tx)

    // assert is long 
    let userAccount = await CH_program.account.user.fetch(user_account); 
    let positions = await CH_program.account.userPositions.fetch(
      userAccount.positions as web3.PublicKey
    );
    let position = positions.positions[0];
    assert(position.baseAssetAmount.gt(drift.ZERO))    
  })

  it("closes long and goes short when mark > oracle", async () => {
    const market = clearingHouse.getMarket(marketIndex); 
    const solUsd = market.amm.oracle;

    // mark > oracle => longs pays shorts 
    await update_twaps(1.0, 1.04); 

    // view_market_state()

    let ix = vault_program.instruction.updatePosition(
      marketIndex,
      authority_b,
      {
        accounts: {
          authority: authority, 
          userPositions: user_positions,
          
          state: clearingHouseStatePk,
          user: user_account,
          markets: clearingHouseState.markets,
          tradeHistory: clearingHouseState.tradeHistory,
          fundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
          fundingRateHistory: clearingHouseState.fundingRateHistory,
          oracle: solUsd,
          clearingHouseProgram: CH_program.programId,
        }
      }
    )

    let tx = new web3.Transaction().add(ix);
    
    // let resp = await provider.simulate(tx);
    // console.log(resp)

    await provider.send(tx)

    // assert is short
    let userAccount = await CH_program.account.user.fetch(user_account); 
    let positions = await CH_program.account.userPositions.fetch(
      userAccount.positions as web3.PublicKey
    );
    let position = positions.positions[0];
    assert(position.baseAssetAmount.lt(drift.ZERO))    

  })

  it('withdraws from the vault', async () => {
    const market = clearingHouse.getMarket(marketIndex); 
    const solUsd = market.amm.oracle;

    var user_vault_balance = await get_token_balance(user_vault_ata)
    var user_usdc_balance = await get_token_balance(userUSDCAccount.publicKey)

    let burn_amount = user_vault_balance; 

    let ix = vault_program.instruction.withdraw(
      burn_amount, //.div(new BN(100)), // 1% widthdraw 
      marketIndex, 
      authority_b,
      {
        accounts: {
          owner: provider.wallet.publicKey, 
          userVaultAta: user_vault_ata, 
          userCollateralAta: userUSDCAccount.publicKey,
          vaultCollateralAta: vault_collateral,
          
          vaultMint: vault_mint, 
          vaultState: vault_state, 
          
          collateralVault: clearingHouseState.collateralVault,
          collateralVaultAuthority: clearingHouseState.collateralVaultAuthority,
          depositHistory: clearingHouseState.depositHistory,
          insuranceVault: clearingHouseState.insuranceVault,
          insuranceVaultAuthority: clearingHouseState.insuranceVaultAuthority,
          tokenProgram: token.TOKEN_PROGRAM_ID,
          updatePosition: {
            authority: authority, 
            userPositions: user_positions,
            state: clearingHouseStatePk,
            user: user_account,
            markets: clearingHouseState.markets,
            tradeHistory: clearingHouseState.tradeHistory,
            fundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
            fundingRateHistory: clearingHouseState.fundingRateHistory,
            oracle: solUsd,
            clearingHouseProgram: CH_program.programId,
          },
        }
      }
    );
    
    let tx = new web3.Transaction().add(ix);

    // let resp = await provider.simulate(tx);
    // console.log(resp)

    await provider.send(tx);

    var user_usdc_balance_end = await get_token_balance(userUSDCAccount.publicKey)
    assert(user_usdc_balance_end.gt(user_usdc_balance)) // got more USDC

    var user_vault_balance_end = await get_token_balance(user_vault_ata)
    assert(user_vault_balance_end.eq(user_vault_balance.sub(burn_amount))); // less vault tokens 

  })
});
