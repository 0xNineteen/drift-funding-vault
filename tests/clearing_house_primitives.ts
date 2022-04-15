import * as anchor from '@project-serum/anchor';
import {Program} from '@project-serum/anchor';
import {DriftVault} from '../target/types/drift_vault';

import {Pyth} from '../target/types/pyth';
import PythIDL from '../target/idl/pyth.json';

import {ClearingHouse} from '../deps/protocol-v1/target/types/clearing_house';
import ClearingHouseIDL from '../deps/protocol-v1/target/idl/clearing_house.json';

import * as token from '@solana/spl-token';
import * as web3 from '@solana/web3.js';

import {assert} from 'chai';
import {Keypair, PublicKey} from '@solana/web3.js';
import {BN} from '@project-serum/anchor';

import {mockUSDCMint, mockUserUSDCAccount, mockOracle} from './testHelpers';

import * as drift from '../deps/protocol-v1/sdk/src';
import {Admin} from '../deps/protocol-v1/sdk/src';
import {
  MARK_PRICE_PRECISION,
  PositionDirection,
  ZERO,
} from '../deps/protocol-v1/sdk/lib';

import {getFeedData, setFeedPrice} from '../ts/mockPythUtils';

import * as fs from 'fs';

const clearingHousePublicKey = new PublicKey(
    'AsW7LnXB9UA1uec9wi9MctYTgTz7YH9snhxd16GsFaGX',
);
const pythPublicKey = new PublicKey(
    '6bgJrRngVsFzCFkjd5PkVKMtb1C3JXgYnEFLhkJPtnEp',
);

describe('drift_vault', () => {
  const provider = anchor.Provider.env();
  const connection = provider.connection;
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
      clearingHousePublicKey,
  );

  const usdcAmount = new BN(100_000 * 10 ** 6);

  const sqrtk = 1e8;
  const PAIR_AMT = sqrtk;
  const ammInitialQuoteAssetAmount = new BN(PAIR_AMT).mul(MARK_PRICE_PRECISION);
  const ammInitialBaseAssetAmount = new BN(PAIR_AMT).mul(MARK_PRICE_PRECISION);

  const marketIndex = new BN(0);

  // oracle price: base/quote amm equal thus price = 1 to match
  const usdcSolPrice = 1;

  // setup programs + fake market
  before(async () => {
    // setup other programs
    CH_program = new anchor.Program(
            ClearingHouseIDL as anchor.Idl,
            clearingHousePublicKey,
            provider,
    ) as Program<ClearingHouse>;

    pyth_program = new anchor.Program(
            PythIDL as anchor.Idl,
            pythPublicKey,
            provider,
    ) as Program<Pyth>;

    usdcMint = await mockUSDCMint(provider);
    userUSDCAccount = await mockUserUSDCAccount(usdcMint, usdcAmount, provider);
    await clearingHouse.initialize(usdcMint.publicKey, true);
    await clearingHouse.subscribeToAll();

    const solUsd = await mockOracle(usdcSolPrice, -6);
    // const periodicity = new BN(60 * 60); // 1 HOUR
    const periodicity = new BN(1); // 1 SECOND

    // fund the market collateral to pay for funding rates
    const clearingHouseState = clearingHouse.getStateAccount();
    const fund_amm_ix = await token.Token.createMintToInstruction(
        token.TOKEN_PROGRAM_ID,
        usdcMint.publicKey,
        clearingHouseState.collateralVault,
        provider.wallet.publicKey,
        [],
        100_000 * 10 ** 6,
    );
    await provider.send(new web3.Transaction().add(fund_amm_ix));

    await clearingHouse.initializeMarket(
        marketIndex,
        solUsd,
        ammInitialBaseAssetAmount,
        ammInitialQuoteAssetAmount,
        periodicity,
        drift.PEG_PRECISION,
    );
  });

  after(async () => {
    await clearingHouse.unsubscribe();
  });

  it('does something drift', async () => {
    // call initialize_vault instruction with a payer
    // user is already created
    // create a new open-orders drift account

    const [userAccountPublicKey, userAccountPublicKeyNonce] =
            await drift.getUserAccountPublicKeyAndNonce(
                CH_program.programId,
                provider.wallet.publicKey,
            );

    const optionalAccounts = {
      whitelistToken: false,
    };

    // create new drift account for provider.wallet
    const userPositions = web3.Keypair.generate();
    const ch_state_pk = await clearingHouse.getStatePublicKey();
    await CH_program.rpc.initializeUserWithExplicitPayer(
        userAccountPublicKeyNonce,
        optionalAccounts,
        {
          accounts: {
            user: userAccountPublicKey, // CH PDA
            state: ch_state_pk, // CH
            userPositions: userPositions.publicKey, // PK
            authority: provider.wallet.publicKey, // PK
            payer: provider.wallet.publicKey, // PK
            rent: web3.SYSVAR_RENT_PUBKEY,
            systemProgram: web3.SystemProgram.programId,
          },
          signers: [userPositions],
        },
    );

    // deposit collateral
    const _depositAmount = 1_000;
    const depositAmount = new BN(_depositAmount * 10 ** 6);

    const clearingHouseState = clearingHouse.getStateAccount();

    // user amount => collateral account for provider.wallet
    await CH_program.rpc.depositCollateral(depositAmount, {
      accounts: {
        user: userAccountPublicKey,
        userPositions: userPositions.publicKey,
        authority: provider.wallet.publicKey,
        userCollateralAccount: userUSDCAccount.publicKey, // !
        // CH things
        state: ch_state_pk,
        collateralVault: clearingHouseState.collateralVault,
        markets: clearingHouseState.markets,
        depositHistory: clearingHouseState.depositHistory,
        fundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
        tokenProgram: token.TOKEN_PROGRAM_ID,
      },
    });

    const userAccount = await CH_program.account.user.fetch(userAccountPublicKey);
    // console.log(userAccount)
    assert(userAccount.collateral.eq(depositAmount));

    // view current market conditions
    const solUsd = clearingHouse.getMarket(marketIndex).amm.oracle;
    const pythClient = new drift.PythClient(connection);
    var market = clearingHouse.getMarket(marketIndex);
    // get oracle/mark price
    var solUsdcData = await getFeedData(pyth_program, solUsd);
    var currentMarketPrice = drift.calculateMarkPrice(market);
    console.log('sol usdc price (mark):', currentMarketPrice.toString());
    console.log('sol usdc price (oracle):', solUsdcData.price);
    // compute funding rate
    var estimated_funding = await drift.calculateEstimatedFundingRate(
        market,
        await pythClient.getOraclePriceData(solUsd),
        new BN(1),
        'interpolated',
    );
    console.log('estimated funding:', estimated_funding.toString());

    // go long on the mock market (USDC/SOL)
    console.log('going long...');

    const quote_amount_in = depositAmount;
    const limitPrice = new BN(0); // yolo
    const optionalAccounts_position = {
      discountToken: false,
      referrer: false,
    };

    await CH_program.rpc.openPosition(
        drift.PositionDirection.LONG,
        quote_amount_in,
        marketIndex, // only market = 0
        limitPrice,
        optionalAccounts_position,
        {
          accounts: {
            // user stuff
            state: ch_state_pk,
            user: userAccountPublicKey,
            userPositions: userPositions.publicKey,
            authority: provider.wallet.publicKey,
            // market oracle PK
            oracle: solUsd,
            // CH stuff
            markets: clearingHouseState.markets,
            tradeHistory: clearingHouseState.tradeHistory,
            fundingPaymentHistory: clearingHouseState.fundingPaymentHistory,
            fundingRateHistory: clearingHouseState.fundingRateHistory,
          },
        },
    );

    const positions = await CH_program.account.userPositions.fetch(
            userAccount.positions as web3.PublicKey,
    );
    const position = positions.positions[0];
    assert(depositAmount.eq(position.quoteAssetAmount));

    // re-compute funding rate (should change bc of mark/oracle difference)
    var market = clearingHouse.getMarket(marketIndex); // update market data post trade

    // await setFeedPrice(pyth_program, solUsdcData.price * 1.02, solUsd) // update oracle UP
    // await setFeedPrice(pyth_program, solUsdcData.price * 0.95, solUsd) // update oracle DOWN

    // get oracle/mark price
    var solUsdcData = await getFeedData(pyth_program, solUsd);
    var currentMarketPrice = drift.calculateMarkPrice(market); // AMM xy = k
    console.log('sol usdc price (mark):', currentMarketPrice.toString());
    console.log('sol usdc price (oracle):', solUsdcData.price);

    // compute funding rate
    var estimated_funding = await drift.calculateEstimatedFundingRate(
        market,
        await pythClient.getOraclePriceData(solUsd),
        new BN(1),
        'interpolated',
    );
    console.log('estimated funding:', estimated_funding.toString());
  });
});
