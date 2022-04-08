import { PublicKey } from '@solana/web3.js';
import * as anchor from '@project-serum/anchor';

export async function getConfigPublicKeyAndConfig(
	programId: PublicKey
): Promise<[PublicKey, number]> {
	return await PublicKey.findProgramAddress(
		[Buffer.from(anchor.utils.bytes.utf8.encode('drift_client'))],
		programId
	);
}

export async function getCollateralVaultPublicKeyAndConfig(
	programId: PublicKey
): Promise<[PublicKey, number]> {
	return await PublicKey.findProgramAddress(
		[Buffer.from(anchor.utils.bytes.utf8.encode('collateral_vault'))],
		programId
	);
}

export async function getCollateralVaultAuthorityPublicKeyAndConfig(
	programId: PublicKey
): Promise<[PublicKey, number]> {
	const collateralVaultPublicKey = (
		await getCollateralVaultPublicKeyAndConfig(programId)
	)[0];
	return await PublicKey.findProgramAddress(
		[collateralVaultPublicKey.toBuffer()],
		programId
	);
}

export async function getClearingHouseAuthorityPublicKeyAndConfig(
	programId: PublicKey,
	clearingHouseProgramId: PublicKey
): Promise<[PublicKey, number]> {
	return await PublicKey.findProgramAddress(
		[clearingHouseProgramId.toBuffer()],
		programId
	);
}
