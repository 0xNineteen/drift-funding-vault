# Capture Drift Funding Rates On-Chain

## Program API 

- `initialize_vault`: initialize a new vault 
- `deposit`: deposit collateral (usdc) into vault and get vault tokens 
- `withdraw`: withdraw deposited collateral from vault by burning vault tokens 
- `update_position`: update the vault's position (can be called by anyone)
    - if the funding rate means the shorts pays the longs => will go long 
    - if the funding rate means the longs pays the shorts => will go short 

## Tests

- `test/`
    - `drift_vault.ts`: main vault tests
        -  ✔ initializes the vault (500ms)
        - ✔ deposits into vault (545ms)
        - ✔ opens a long when mark < oracle (1539ms)
        - ✔ closes long and goes short when mark > oracle (1555ms)
        - ✔ withdraws from the vault (510ms)
        - ✔ re-deposits in the vault, goes long, captures funding, closes for profit (15625ms)
    - `clearing_house_primitives`: example tests of how to interact directly with the clearing house via API 

other files are copy-pasta'd from the `cpi-examples` repo (see References).

## Setup 

- change `[provider]` `wallet` path in `Anchor.toml`
- set correct anchor version: `avm use 0.22.0`
- `bash setup.sh`: build the clearing house, sdk, mock-pyth program, and vault program 

## Notes

- `protocol-v1` is modified to include a new method `update_twaps` which allows anyone to set the oracle / mark twaps to whatever they want -- was useful for unit testing different funding rates 

## References 
- largely based off of drift's [cpi-example](https://github.com/drift-labs/cpi-example)