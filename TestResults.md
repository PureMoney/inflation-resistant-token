# Test Results
This doc expounds on some of the more interesting tests done on the IRMA program.

## Minting Test
Minting is very simple and un-interesting. The mint price is dictated by two outside measures, the reserve backing 
stablecoin market price and Truflation's inflation measure. While a reserve backing's market price should not deviate much 
from 1.0 USD, during times of high inflation we expect it to veer away from 1.0 USD, and this will vary depending on 
how much a stablecoin's users trust it. 

Assuming that inflation is below 2% when we set the IRMA loose in Solana mainnet, then IRMA will start out just like any other 
stablecoin and its mint price will be equal to 1.0 times every stablecoin in its reserve. (All reserves start out at zero at time = 0; 
therefore redemption price is zero and there is nothing to redeem.) The first buyers or miners of IRMA will start building up the reserves. 
The very first buyer of IRMA for USDT, for example, for 100 USDT say, would receive 100 IRMA and would therefore also cause the 
redemption price to be set at 1.0 USDT. As time goes, therefore, as long as inflation stays below 2%, IRMA price with respect 
to a backing stablecoin will be 1.0 unit of that stablecoin.

The functional test we want to do for minting starts with initial conditions such that minting price equals redemption price for 
every backing stablecoin. While mints and redemptions are going on, we would then suddenly raise the minting price, simulating 
a sudden rise of inflation beyond 2%. We expect the redemption price to go up as orders continue to come in, thereby approaching 
the mint price in time. That's it, that would be the functionality test for minting. However, at this point, we have only run 
unit tests on the mint_irma() function. This section of the doc will add more info later.

## "Total Redemption" Test
The redemption functionality test is much more interesting than any minting test we could think of. 

The IRMA program is designed to be fungible with respect to all backing stablecoins. In other words, once minted, each IRMA does 
not really care which stablecoin it is backed by, and it shouldn't. This is an important requirement. This implies that it should be OK 
to mint IRMA using USDT and then use the newly minted IRMA to redeem ANOTHER stablecoin, say USDC.

An arbitrageur, having detected a price difference between, say, USDT and USDC out there in the market that does not match the 
implied exchange rates of USDT and USDC in the IRMA system, can take advantage of the discrepancy. This arbitrageur can essentially 
use the IRMA system to exchange USDT for USDC, for example.

The IRMA system keeps track of the reserve backing total for each reserve stablecoin and also the amount of total IRMA in circulation 
for that stablecoin. When minting, the amount of stablecoin paid is added to the reserve backing total and the amount of IRMA minted is added 
to the total count of IRMA in circulation specific to that stablecoin. When redeeming, on the other hand, the naive way is to simply 
subtract the redeemed stablecoin amount from that stablecoin's reserve backing total, and also subtract the amount of IRMA returned 
from the total count of IRMA in circulation (specific to that stablecoin). This allows minting to adjust the redemption price towards 
the mint price; but redemptions would not affect the redemption price (because redemptions subtract from the IRMA in circulation 
according to the redemption price rather than the mint price).

The fungibility requirement can devastate IRMA if the redemption function simply subtracted both the input IRMA amount 
(from the total amount in circulation) and the redeemed stablecoin amount (from the reserve backing total). The IRMA program has to protect 
IRMA from possible runs because of the fungibility requirement. The naive way to redeem does not affect the redemption price, 
therefore redemptions can continue unabated at the same redemption price. To protect IRMA, the IRMA program must provide 
a dis-incentive for excessive redemptions. The following items provide this dis-incentive:

1. Redemption price is less than the mint price most of the time. This dis-incentive disappears at times.
2. The redemption function limits the per-redemption amount to 100K IRMA, but does not restrict the number of times redemptions can occur.
3. The overall total IRMA in circulation is tracked, but not necessarily for each reserve stablecoin.

What item number 3 means is that a "run" can still occur for a reserve stablecoin (the IRMA system can run out of a particular stablecoin), 
but users can continue to redeem another reserve stablecoin. This protects IRMA by calculating all deviations from the mint prices and determining 
the redemption price with the most deviation from the mint price. The IRMA program then "assigns" the amount of IRMA for redemption to this 
stablecoin with the most deviation.

The redemption test results illustrated below show how the IRMA redemption function works. 

When inflation hits above 2%, the mint prices are adjusted according to inflation and according to the price of each reserve stablecoin. Here
we simply pick mint prices vastly different from each other in order to see clearly what would happen with the redemption prices in the face
of vastly different target mint prices. 

![IRMA_MintPrice_with_labels](https://github.com/user-attachments/assets/fe13f5a2-4ee5-471a-97d7-6faf1f0b04b9)

The graph below shows exactly what happens when USDT is redeemed millions of times at 100K each redemption. Note that there is no minting
going on at the same time; in fact, no other transactions are being fed to the IRMA program except the large number of redemptions, one after 
another in a loop, until an error occurs. Notice how the redemption price for USDT goes down as the redemptions occur, while the redemption
prices for the other stablecoins increase. This should be an effective dis-incentive.

![IRMA_RedemptionPrice_with_labels](https://github.com/user-attachments/assets/8d71dfd8-d008-4455-83f5-c3c169dfae95)

We are redeeming USDT only, so only the USDT reserve total goes down:

![IRMA_Reserve_with_labels](https://github.com/user-attachments/assets/26a72865-34d9-4a45-bfc1-d080df739f18)

On the other hand, IRMA in circulation for each stablecoin reserve changes for every redemption. Because redemption price is simply
reserve total divided by IRMA in circulation for a stablecoin, notice how the redemption price in the second graph above changes
to approach the mint price for each stablecoin, even when no minting is going on.

![IRMA_Circulation_with_labels](https://github.com/user-attachments/assets/f73d90a7-80ae-43be-b07d-2154eaf3a732)


