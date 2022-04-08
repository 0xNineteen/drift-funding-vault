import * as anchor from '@project-serum/anchor';
import { BN, Program } from '@project-serum/anchor';
import {
	ClearingHouse,
	getUserAccountPublicKey,
	getUserAccountPublicKeyAndNonce,
	PositionDirection,
	UserAccount,
	UserPositionsAccount,
} from '../deps/protocol-v1/sdk';
import {
	Keypair,
	PublicKey,
	SYSVAR_RENT_PUBKEY,
	TransactionSignature,
} from '@solana/web3.js';
import { TOKEN_PROGRAM_ID } from '@solana/spl-token';
import {
	getClearingHouseAuthorityPublicKeyAndConfig,
	getCollateralVaultAuthorityPublicKeyAndConfig,
	getCollateralVaultPublicKeyAndConfig,
	getConfigPublicKeyAndConfig,
} from './addresses';

export type Config = {
	admin: PublicKey;
	collateralVault: PublicKey;
	authority: PublicKey;
};

export class DriftClient {
	program: Program;
	clearingHouse: ClearingHouse;

	public constructor(program: Program, clearingHouse: ClearingHouse) {
		this.program = program;
		this.clearingHouse = clearingHouse;
		if (!this.clearingHouse.isSubscribed) {
			throw new Error('ClearingHouse must be subscribed');
		}
	}

	public async getConfig(): Promise<Config> {
		return await this.program.account.config.fetch(
			await this.getConfigPublicKey()
		);
	}

	public async getConfigPublicKey(): Promise<PublicKey> {
		return (await getConfigPublicKeyAndConfig(this.program.programId))[0];
	}

	public async getCollateralVaultPublicKey(): Promise<PublicKey> {
		return (
			await getCollateralVaultPublicKeyAndConfig(this.program.programId)
		)[0];
	}

	public async getCollateralVaultAuthorityPublicKey(): Promise<PublicKey> {
		return (
			await getCollateralVaultAuthorityPublicKeyAndConfig(
				this.program.programId
			)
		)[0];
	}

	public async getAuthorityPublicKey(): Promise<PublicKey> {
		return (
			await getClearingHouseAuthorityPublicKeyAndConfig(
				this.program.programId,
				this.clearingHouse.program.programId
			)
		)[0];
	}

	public async getClearingHouseUserAccountPublicKey(): Promise<PublicKey> {
		return await getUserAccountPublicKey(
			this.clearingHouse.program.programId,
			await this.getAuthorityPublicKey()
		);
	}

	public async getUserAccount(): Promise<UserAccount> {
		const userAccountPublicKey =
			await this.getClearingHouseUserAccountPublicKey();
		return this.clearingHouse.program.account.user.fetch(userAccountPublicKey);
	}

	public async getUserPositionsAccount(): Promise<UserPositionsAccount> {
		const userAccount = await this.getUserAccount();
		return this.clearingHouse.program.account.userPositions.fetch(
			userAccount.positions
		);
	}

	public async initialize(): Promise<TransactionSignature> {
		const [configPublicKey, configNonce] = await getConfigPublicKeyAndConfig(
			this.program.programId
		);

		const [collateralVaultPublicKey, collateralVaultNonce] =
			await getCollateralVaultPublicKeyAndConfig(this.program.programId);

		const authority = await this.getAuthorityPublicKey();

		return await this.program.rpc.initialize(
			configNonce,
			collateralVaultNonce,
			{
				accounts: {
					admin: this.program.provider.wallet.publicKey,
					config: configPublicKey,
					authority,
					collateralMint: this.clearingHouse.getStateAccount().collateralMint,
					collateralVault: collateralVaultPublicKey,
					clearingHouseState: await this.clearingHouse.getStatePublicKey(),
					rent: SYSVAR_RENT_PUBKEY,
					systemProgram: anchor.web3.SystemProgram.programId,
					tokenProgram: TOKEN_PROGRAM_ID,
				},
			}
		);
	}

	public async initializeUser(): Promise<TransactionSignature> {
		const authority = await this.getAuthorityPublicKey();
		const [userAccountPublicKey, userAccountPublicKeyNonce] =
			await getUserAccountPublicKeyAndNonce(
				this.clearingHouse.program.programId,
				authority
			);
		const userPositions = new Keypair();
		return await this.program.rpc.initializeUser(userAccountPublicKeyNonce, {
			accounts: {
				admin: this.program.provider.wallet.publicKey,
				config: await this.getConfigPublicKey(),
				clearingHouseState: await this.clearingHouse.getStatePublicKey(),
				clearingHouseUser: userAccountPublicKey,
				clearingHouseUserPositions: userPositions.publicKey,
				authority,
				rent: SYSVAR_RENT_PUBKEY,
				systemProgram: anchor.web3.SystemProgram.programId,
				clearingHouseProgram: this.clearingHouse.program.programId,
			},
			signers: [userPositions],
		});
	}

	public async depositCollateral(
		amount: BN,
		collateralAccount: PublicKey
	): Promise<TransactionSignature> {
		const clearingHouseState = this.clearingHouse.getStateAccount();
		const userAccount = await this.getUserAccount();
		return await this.program.rpc.depositCollateral(amount, {
			accounts: {
				admin: this.program.provider.wallet.publicKey,
				adminCollateralAccount: collateralAccount,
				config: await this.getConfigPublicKey(),
				clearingHouseState: await this.clearingHouse.getStatePublicKey(),
				clearingHouseUser: await this.getClearingHouseUserAccountPublicKey(),
				clearingHouseUserPositions: userAccount.positions,
				clearingHouseMarkets: clearingHouseState.markets,
				clearingHouseCollateralVault: clearingHouseState.collateralVault,
				clearingHouseDepositHistory: clearingHouseState.depositHistory,
				clearingHouseFundingPaymentHistory:
					clearingHouseState.fundingPaymentHistory,
				collateralVault: await this.getCollateralVaultPublicKey(),
				authority: await this.getAuthorityPublicKey(),
				tokenProgram: TOKEN_PROGRAM_ID,
				clearingHouseProgram: this.clearingHouse.program.programId,
			},
		});
	}

	public async withdrawCollateral(
		amount: BN,
		collateralAccount: PublicKey
	): Promise<TransactionSignature> {
		const clearingHouseState = this.clearingHouse.getStateAccount();
		const userAccount = await this.getUserAccount();
		return await this.program.rpc.withdrawCollateral(amount, {
			accounts: {
				admin: this.program.provider.wallet.publicKey,
				adminCollateralAccount: collateralAccount,
				config: await this.getConfigPublicKey(),
				clearingHouseState: await this.clearingHouse.getStatePublicKey(),
				clearingHouseUser: await this.getClearingHouseUserAccountPublicKey(),
				clearingHouseUserPositions: userAccount.positions,
				clearingHouseMarkets: clearingHouseState.markets,
				clearingHouseCollateralVault: clearingHouseState.collateralVault,
				clearingHouseCollateralVaultAuthority:
					clearingHouseState.collateralVaultAuthority,
				clearingHouseInsuranceVault: clearingHouseState.insuranceVault,
				clearingHouseInsuranceVaultAuthority:
					clearingHouseState.insuranceVaultAuthority,
				clearingHouseDepositHistory: clearingHouseState.depositHistory,
				clearingHouseFundingPaymentHistory:
					clearingHouseState.fundingPaymentHistory,
				collateralVault: await this.getCollateralVaultPublicKey(),
				authority: await this.getAuthorityPublicKey(),
				tokenProgram: TOKEN_PROGRAM_ID,
				clearingHouseProgram: this.clearingHouse.program.programId,
			},
		});
	}

	public async openPosition(
		direction: PositionDirection,
		amount: BN,
		marketIndex: BN,
		limitPrice?: BN,
		discountToken?: PublicKey,
		referrer?: PublicKey
	): Promise<TransactionSignature> {
		if (limitPrice == undefined) {
			limitPrice = new BN(0); // no limit
		}

		const optionalAccounts = {
			discountToken: false,
			referrer: false,
		};
		const remainingAccounts = [];
		if (discountToken) {
			optionalAccounts.discountToken = true;
			remainingAccounts.push({
				pubkey: discountToken,
				isWritable: false,
				isSigner: false,
			});
		}
		if (referrer) {
			optionalAccounts.referrer = true;
			remainingAccounts.push({
				pubkey: referrer,
				isWritable: true,
				isSigner: false,
			});
		}

		const priceOracle = this.clearingHouse.getMarket(marketIndex).amm.oracle;

		const clearingHouseState = this.clearingHouse.getStateAccount();
		const userAccount = await this.getUserAccount();
		return await this.program.rpc.openPosition(
			direction,
			amount,
			marketIndex,
			limitPrice,
			optionalAccounts,
			{
				accounts: {
					admin: this.program.provider.wallet.publicKey,
					config: await this.getConfigPublicKey(),
					clearingHouseState: await this.clearingHouse.getStatePublicKey(),
					clearingHouseUser: await this.getClearingHouseUserAccountPublicKey(),
					clearingHouseUserPositions: userAccount.positions,
					clearingHouseMarkets: clearingHouseState.markets,
					oracle: priceOracle,
					clearingHouseTradeHistory: clearingHouseState.tradeHistory,
					clearingHouseFundingPaymentHistory:
						clearingHouseState.fundingPaymentHistory,
					clearingHouseFundingRateHistory:
						clearingHouseState.fundingRateHistory,
					authority: await this.getAuthorityPublicKey(),
					clearingHouseProgram: this.clearingHouse.program.programId,
				},
			}
		);
	}

	public async closePosition(
		marketIndex: BN,
		discountToken?: PublicKey,
		referrer?: PublicKey
	): Promise<TransactionSignature> {
		const optionalAccounts = {
			discountToken: false,
			referrer: false,
		};
		const remainingAccounts = [];
		if (discountToken) {
			optionalAccounts.discountToken = true;
			remainingAccounts.push({
				pubkey: discountToken,
				isWritable: false,
				isSigner: false,
			});
		}
		if (referrer) {
			optionalAccounts.referrer = true;
			remainingAccounts.push({
				pubkey: referrer,
				isWritable: true,
				isSigner: false,
			});
		}

		const priceOracle = this.clearingHouse.getMarket(marketIndex).amm.oracle;

		const clearingHouseState = this.clearingHouse.getStateAccount();
		const userAccount = await this.getUserAccount();
		return await this.program.rpc.closePosition(marketIndex, optionalAccounts, {
			accounts: {
				admin: this.program.provider.wallet.publicKey,
				config: await this.getConfigPublicKey(),
				clearingHouseState: await this.clearingHouse.getStatePublicKey(),
				clearingHouseUser: await this.getClearingHouseUserAccountPublicKey(),
				clearingHouseUserPositions: userAccount.positions,
				clearingHouseMarkets: clearingHouseState.markets,
				oracle: priceOracle,
				clearingHouseTradeHistory: clearingHouseState.tradeHistory,
				clearingHouseFundingPaymentHistory:
					clearingHouseState.fundingPaymentHistory,
				clearingHouseFundingRateHistory: clearingHouseState.fundingRateHistory,
				authority: await this.getAuthorityPublicKey(),
				clearingHouseProgram: this.clearingHouse.program.programId,
			},
		});
	}
}
