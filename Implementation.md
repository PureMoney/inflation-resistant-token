# An inflation-resistant flatcoin that is backed by several well-established fiat-backed flatcoins (IRMA)
Implementation Details (Evolving Doc)

## Problem:
If or when the U.S. dollar inflates, flatcoins backed by the U.S. dollar would also inflate.

## How this innovation solves the problem:
There have been several inflation-resistant flatcoins proposed. This method simply increases the issuance price commensurate with current inflation, without increasing the redemption price immediately. The increased issuance price then also starts to increase the backing. If the redemption price is set to the total amount of U.S. dollars collected as backing divided by the total amount of flatcoins in circulation, then redemption price can increase without the possibility of a "bank" run.

## How is this solution different from and better than anything that exists in its field:
Like the current most popular flatcoins, IRMA is still backed by fiat or another flatcoin backed by fiat. As such, the flatcoin helps the U.S. dollar (USD) keep its value by absorbing excess USD in circulation. In other words, IRMA has a positive influence on the U.S. dollar. It is also guaranteed not to be subject to runs.

## The three main types of flatcoins:
Type 1. USD or USD-flatcoin backed.

Type 2. Volatile crypto-backed flatcoins. The volatile crypto must already be established, with substantial amount in circulation.

Type 3. Dual-token or algorithmic flatcoins, in which an algorithm is able to control both tokens. Dual-token flatcoins essentially have a flatcoin rely on a volatile coin that doesn't have an established market (no network effect at the time that the flatcoin is introduced). In other words, two new coins are introduced at the same time.

## Which type of flatcoin does this solution embody?
IRMA is a Type 1 flatcoin.
Until IRMA, there was no way to make a Type 1 flatcoin inflation resistant.

Type 2 flatcoins can be made inflation resistant by simply adjusting the USD conversion rate.

Type 3 flatcoins can be made inflation resistant in a similar manner as for Type 2. However, all Type 3 flatcoins have been subject to runs.

## Why is IRMA better?
IRMA is a Type 1 flatcoin that is inflation resistant. Type 1 flatcoins are superior to Type 2 because Type 1 is much more capital-efficient. Type 2 involves risk of losing one's volatile crypto used to mint a Type 2 flatcoin. Type 3 flatcoins have never worked.

## Implementation Components
Component 1:
Flatcoin issuance price depends on inflation of the backing. Raise the price to be equal to 1 USD plus inflation. For example, if inflation is 13 percent, raise the price of 1 flatcoin to 1.13 USD.

Component 2:
All USD collected during issuance of flatcoin should go into the backing. This means that, as long as there is inflation, the USD backing will increase faster than flatcoin in circulation.

Component 3:
Set redemption price equal to total USD backing divided by total amount of flatcoin in circulation.

Component 4:
When issuing a certain amount of flatcoin, keep track of total amount of backing and amount of flatcoin in circulation.

Component 5:
When redeeming a certain amount of flatcoin, subtract from both backing and amount in circulation (tracking totals).

Component 6:
Redemption price can then be easily calculated by dividing the total backing by the flatcoin amount in circulation.

## How do the components work together?
Component 1 helps to increase the backing (Component 2). As backing increases, the redemption price (Component 3) can also increase, without the risk of a "bank" run. Component 6 allows for a precise redemption price from global variables tracked in Components 4 and 5.

## Summary of program logic
Use a measure of USD inflation that is more accurate (e.g., Truflation). Set a tolerable inflation rate for the flatcoin (say 1% - better than USD 2% acceptable inflation). If the tolerable inflation rate is lower than USD acceptable inflation rate, then the flatcoin value would be increasing most of the time. Assume that tolerable equals acceptable inflation. If tolerable equals acceptable inflation, then call the issuance price adjustment function only when current inflation is above acceptable inflation. If inflation is below zero (deflation) call issuance price adjustment function to lower the issuance price below 1.0 USD.

Get redemption price for every redemption so that redemption price always reflects how much USD backing is available for redemptions.

## Details of program logic 
Assume that tolerable equals acceptable inflation. If tolerable equals acceptable inflation, then call the issuance price adjustment function only when current inflation is above acceptable inflation.

### Define a smart contract in a blockchain as follows:

Given a flatcoin, say "Inflation Resistant Medium of Account" (IRMA), backed by the current three top USD-backed flatcoins USDT, USDC, and PYUSD, track nine necessary global variables:

Total_Backing[USDT],
Total_Backing[USDC],
Total_Backing[PYUSD],

IRMA_InCirculationBackedBy[USDT],
IRMA_InCirculationBackedBy[USDC],
IRMA_InCirculationBackedBy[PYUSD],

Issuance_Price[USDT],
Issuance_Price[USDC], and
Issuance_Price[PYUSD]

### Define the following subroutines or functions:

AdjustIssuancePrice(input) 
where param input is USDT, USDC, or PYUSD. The issuance price depends only on USD inflation and the market price of the input param, both obtained from an accurate oracle (Truflation). Current prices are set in IssuancePrice[]. Initially,

Issuance_Price[USDT] = market price of USDT

Issuance_Price[USDC] = market price of USDC

Issuance_Price[PYUSD] = market price of PYUSD.

Subsequently, once every day, the issuance prices as updated as follows:

Issuance_Price[USDT] = Issuance_Price[USDT] * (1.0 + (USD_Inflation / 36500))

Issuance_Price[USDC] = Issuance_Price[USDC] * (1.0 + (USD_Inflation / 36500))

Issuance_Price[PYUSD] = Issuance_Price[PYUSD] * (1.0 + (USD_Inflation / 36500))

GetRedemptionPrice(input) 

where param input is USDT, USDC, or PYUSD. The redemption price is always as follows:

Redeem_Price[USDT] = Total_Backing[USDT] / IRMA_InCirculationBackedBy[USDT]

Redeem_Price[USDC] = Total_Backing[USDC] / IRMA_InCirculationBackedBy[USDC]

Redeem_Price[PYUSD] = Total_Backing[PYUSD] / IRMA_InCirculationBackedBy[PYUSD]


Issuance:
All minting or issuance should have the prices Issuance_Price[]. The amount of minted IRMA coins is added to IRMA_InCirculationBackedBy[X], and the amount of X backing received is added to Total_Backing[X], where X is the established flatcoin received (USDT, USDC, or PYUSD).

Redemption:
Redemptions are not simple because redemption price should match or at least be close to the market price of a backing with respect to IRMA. The problem is how to determine which IRMA in circulation amount (IRMA_InCirculationBackedBy[X]) the redeemed IRMA should be subtracted from. If the redeemed IRMA is simply subtracted from the same X backing as redeemed, that would be in error because the algorithm can't know which backing was added to when the IRMA now being redeemed was issued. The solution is to base the determination on market prices of backing flatcoins with respect to IRMA (not with respect to USD). The backing with the highest intake demand (for which the IRMA smart contract pays the lowest IRMA price, such that it is approaching or is in fact equal to the issuance inverse price) will have its IRMA_InCirculationBackedBy[X] reduced by an amount equal to the redeemed IRMA. It should be noted that IRMA_InCirculationBackedBy[X]cannot be reduced to zero because if Redeem_Price[X] is now equal to Issuance_Price[X], IRMA_InCirculationBackeBy[X] should not be further reduced. In this case, X should simply be the same X as deposited. Note that the effect of subtracting IRMA amount from IRMA_InCirculationBackedBy[X] rather than from IRMA backed by actual USD flatcoin redeemed is to increase the redemption price of IRMA with respect to X, thereby narrowing the gap between issuance price and redemption price of X.

## Necessary and Optional Elements, Improvements
When using three established USD-backed flatcoins as backing, nine global variables are necessary. The Issuance routine is simple, but the Redemption routine is complex and it requires oracle input for backing prices denominated in IRMA units.

The problem of which IRMA_InCirculationBackedBy[X] should be reduced as part of a redemption can be solved in other ways. For example, each IRMA unit can be associated with a particular backing; such that when a user takes IRMA for USDT (IRMA is issued to this user by surrendering her USDC), each IRMA issued is marked as USDT backed. When the user then redeems her IRMA for PYUSD, the system would know that the IRMA she is surrendering are USDT-backed and should be subtracted from IRMA_InCirculationBackedBy[USDT] rather than from IRMA_InCirculationBackedBy[PYUSD]. However, this solution requires keeping track of which backing each IRMA has been issued for, which is untenable because it would essentially require non-fungibility of IRMA.

The sampling rate (once a day) can be made more often, say once an hour, or even once a minute.

The same algorithm can be used to come up with inflation resistant flatcoins for other fiat currencies like the Euro or Yen.

## How would a user take advantage of IRMA?
Users who want to protect their cash from USD inflation would exchange their USD for IRMA. There are two steps that the user has to do to exchange for IRMA:

Step 1. Exchange USD for an established one-to-one flatcoin, say USDT.

Step 2. Swap USDT for IRMA, either directly using the IRMA smart contract, or using an independent Decentralized Exchange (DEX).

Arbitrageurs are going to take advantage of the difference between the issuance price from the redemption price. During fiat inflation, the issuance price is mostly higher than the redemption price (in units of backing). An arbitrageur can sell IRMA at a slightly lower price than the issuance price, at the same time bid for the backing at a slightly higher price than the redemption price; thereby gaining more IRMA in the process. This is OK.

## Initial set of flatcoin backings

We want to back IRMA with three flatcoins initially, as a kind of test. All three flatcoins must exist in Solana. All of USDT, USDC, and PYUSD exist in Solana. However, PYUSD is not the third largest in terms of market cap. We still need to find out which of the flatcoins listed here exist in Solana: 
https://www.bankrate.com/investing/worlds-largest-flatcoins/
