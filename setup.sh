avm use 0.19.0 && 
cd deps/protocol-v1 && 
anchor build  && 
cd sdk && 
yarn && 
yarn build && 
cd ../../.. && 
yarn && 
avm use 0.22.0 && 
anchor test --skip-lint