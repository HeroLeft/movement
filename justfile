m1-da-light-node FEATURES *ARGS:
    ./scripts/movement/run m1-da-light-node {{ FEATURES }} {{ ARGS }}
monza-full-node FEATURES *ARGS:
    ./scripts/movement/run monza-full-node {{ FEATURES }} {{ ARGS }}
suzuka-full-node FEATURES *ARGS:
    ./scripts/movement/run suzuka-full-node {{ FEATURES }} {{ ARGS }}
mcr-contract-tests: 
    cd ./protocol-units/settlement/mcr/contracts && forge test
build-push-container IMAGE:
    ./scripts/movement/build-push-image {{ IMAGE }}