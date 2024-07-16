// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console} from "forge-std/Test.sol";
import {MockToken} from "../src/MockToken.sol";
import {WETH10} from "../src/WETH10.sol";

contract CounterTest is Test {
    function setUp() public {}

    function testDeploy() public {
        MockToken usdc = new MockToken("Circle", "USDC", 6, 60000000000000, 3600);
        MockToken usdt = new MockToken("Tether", "USDT", 6, 60000000000000, 3600);
        MockToken wbtc = new MockToken("Bitcoin", "WBTC", 8, 100000000, 3600);
        MockToken moveth = new MockToken("MovETH", "MOVETH", 8, 2000000000, 3600);
        WETH10 wmove = new WETH10();
    }
}
