# IRMA: A flatcoin that is backed by several well-established fiat-backed stablecoins
Implementation Details (Evolving Doc)

## Problem:
If or when the U.S. dollar inflates, stablecoins backed by the U.S. dollar would also inflate.

## How this innovation solves the problem:
There have been several inflation-resistant coins (flatcoins) proposed. The IRMA method simply increases the issuance price commensurate with current inflation, without increasing the redemption price immediately. The increased issuance price then also starts to increase the reserve-backing faster than IRMA in circulation. If the redemption price is set to the total amount of stablecoins collected as backing divided by the total amount of IRMA in circulation, then redemption price can increase without the possibility of a "bank" run.

## How is this solution different from and better than anything that exists in its field:
Like the current most popular stablecoins, IRMA is still backed by another stablecoin which, in turn, is backed by fiat. As such, the flatcoin IRMA helps the U.S. dollar (USD) keep its value by absorbing excess USD in circulation. In other words, IRMA has a positive influence on the U.S. dollar. It is also guaranteed not to be subject to runs.

## The three main types of stablecoins:
Type 1. USD backed.

Type 2. Volatile crypto-backed stablecoins. The volatile crypto must already be established, with substantial amount in circulation.

Type 3. Dual-token or algorithmic stablecoins, in which an algorithm is able to control both tokens. Dual-token stablecoins essentially have a stablecoin rely on a volatile coin that doesn't have an established market (no network effect at the time that the stablecoin is introduced). In other words, two new coins are introduced at the same time.

NOTE: The GENIUS Act has changed the definition of stablecoins such that only Type 1 are considered stablecoins. Types 2 and 3 are non-compliant stablecoins.

## Which type of stablecoin does this solution embody?
IRMA is a Type 1 flatcoin; however, it is not reserve-backed directly by USD; instead, it is reserve-backed by multiple well-established stablecoins.
Until IRMA, there was no way to make a Type 1 stablecoin inflation resistant.

Type 2 stablecoins can be made inflation resistant by simply adjusting the USD conversion rate.

Type 3 stablecoins can be made inflation resistant in a similar manner as for Type 2. However, all Type 3 stablecoins have been subject to runs.

## Why is IRMA better?
IRMA is a Type 1 stablecoin that is inflation resistant. Type 1 stablecoins are superior to Type 2 because Type 1 is much more capital-efficient. Type 2 involves risk of losing one's volatile crypto used to mint a Type 2 stablecoin. Type 3 stablecoins have never worked.

## Implementation Components
Component 1:
IRMA issuance price depends on inflation of the backing. Raise the price to be equal to 1 USD plus inflation. For example, if inflation is 13 percent, raise the price of 1 IRMA to 1.13 USD. (In practice, the price is raised to 1.11 in this example because USD inflation below 2% is ignored.)

Component 2:
All USD stablecoins collected during issuance of IRMA should go into the backing. This means that, as long as there is inflation, the stablecoin backing will increase faster than IRMA in circulation.

Component 3:
Set redemption price equal to total stablecoin backing divided by total amount of IRMA in circulation.

Component 4:
When issuing a certain amount of IRMA, keep track of total amount of backing and amount of IRMA in circulation.

Component 5:
When redeeming a certain amount of IRMA, subtract from both backing and amount in circulation (tracking totals). (This is simplified - complications result from the fact that IRMA is fungible - see section on Redemption below.)

Component 6:
Redemption price can then be easily calculated by dividing the total backing by the total amount of IRMA in circulation. (In practice, each stablecoin reserve-backing is tracked separately from other reserves such that, taking USDT as an example, the redemption price for USDT with respect to IRMA is always equal to total USDT in reserves divided by total IRMA in circulation attributed to USDT.)

## How do the components work together?
Component 1 helps to increase the backing (Component 2) faster than IRMA in circulation. As backing increases, the redemption price (Component 3) can also increase, without the risk of a "bank" run. Component 6 allows for a precise redemption price from global variables tracked in Components 4 and 5.

## Summary of program logic
Use a measure of USD inflation that is more accurate (e.g., Truflation). Set a tolerable inflation rate for the stablecoin (say 1% - better than USD 2% acceptable inflation). If the tolerable inflation rate is lower than USD acceptable inflation rate, then the stablecoin value would be increasing most of the time. Assume that tolerable equals acceptable inflation. If tolerable equals acceptable inflation, then call the issuance price adjustment function only when current inflation is above acceptable inflation. If inflation is below zero (deflation) call issuance price adjustment function to lower the issuance price below 1.0 USD.

Get redemption price for every redemption so that redemption price always reflects how much USD backing is available for redemptions.

## Details of program logic 
Assume that tolerable equals acceptable inflation. If tolerable equals acceptable inflation, then call the issuance price adjustment function only when current inflation is above acceptable inflation.

### Define a smart contract in a blockchain as follows:

Given a stablecoin, say "Inflation Resistant Medium of Account" (IRMA), backed by the current three top USD-backed stablecoins USDT, USDC, and PYUSD, track nine necessary global variables:

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

Market_Price[USDT] = market price of USDT with respect to USD

Market_Price[USDC] = market price of USDC with respect to USD

Market_Price[PYUSD] = market price of PYUSD with respect to USD.

Subsequently, once every day, the issuance prices are updated as follows (ensure USD_Inflation is expressed as a per-day rate in these formulas):

Issuance_Price[USDT] = Issuance_Price[USDT] * (1.0 + (USD_Inflation / Market_Price[USDT]))

Issuance_Price[USDC] = Issuance_Price[USDC] * (1.0 + (USD_Inflation / Market_Price[USDC]))

Issuance_Price[PYUSD] = Issuance_Price[PYUSD] * (1.0 + (USD_Inflation / Market_Price[PYUSD]))

GetRedemptionPrice(input) 

where param input is USDT, USDC, or PYUSD. The redemption price is always as follows:

Redeem_Price[USDT] = Total_Backing[USDT] / IRMA_InCirculationBackedBy[USDT]

Redeem_Price[USDC] = Total_Backing[USDC] / IRMA_InCirculationBackedBy[USDC]

Redeem_Price[PYUSD] = Total_Backing[PYUSD] / IRMA_InCirculationBackedBy[PYUSD]


Issuance:
All minting or issuance should have the prices Issuance_Price[]. The amount of minted IRMA coins is added to IRMA_InCirculationBackedBy[X], and the amount of X backing received is added to Total_Backing[X], where X is the established stablecoin received (USDT, USDC, or PYUSD).

Redemption:
Redemptions are not simple because redemption price should match or at least be close to the market price of a backing with respect to IRMA. The problem is how to determine which IRMA in circulation amount (IRMA_InCirculationBackedBy[X]) the redeemed IRMA should be subtracted from. If the redeemed IRMA is simply subtracted from the same X backing as redeemed, that would be in error because the algorithm can't know which backing was added to when the IRMA now being redeemed was issued. The solution is to attribute the redeemed IRMA amount to that stablecoin Y with the largest error (largest difference between issuance or mint price and redemption price). The backing with the largest error will have its IRMA_InCirculationBackedBy[Y] reduced by an amount equal to the redeemed IRMA. It should be noted that IRMA_InCirculationBackedBy[Y] cannot be reduced to zero because if Redeem_Price[Y] is now equal to Issuance_Price[Y], IRMA_InCirculationBackedBy[Y] would not be further reduced. In this case, Y could simply be the same X that was deposited, or the third stablecoin. Note that the effect of subtracting IRMA amount from IRMA_InCirculationBackedBy[Y] rather than from IRMA backed by actual USD stablecoin redeemed (X) is to increase the redemption price of IRMA with respect to Y, thereby narrowing the gap between issuance price and redemption price of Y.

## Necessary and Optional Elements, Improvements
When using three established USD-backed stablecoins as backing, nine global variables are necessary. The Issuance routine is simple, but the Redemption routine is complex and it requires oracle input for backing prices denominated in IRMA units.

The problem of which IRMA_InCirculationBackedBy[X] should be reduced as part of a redemption can be solved in other ways. For example, each IRMA unit can be associated with a particular backing; such that when a user takes IRMA for USDT (IRMA is issued to this user by surrendering her USDC), each IRMA issued is marked as USDT backed. When the user then redeems her IRMA for PYUSD, the system would know that the IRMA she is surrendering are USDT-backed and should be subtracted from IRMA_InCirculationBackedBy[USDT] rather than from IRMA_InCirculationBackedBy[PYUSD]. However, this solution requires keeping track of which backing each IRMA has been issued for, which is untenable because it would essentially require non-fungibility of IRMA.

The sampling rate (once a day) can be made more often, say once an hour, or even once a minute.

The same algorithm can be used to come up with inflation resistant stablecoins for other fiat currencies like the Euro or Yen.

## How would a user take advantage of IRMA?
Users who want to protect their cash from USD inflation would exchange their USD for IRMA. There are two steps that the user has to do to exchange for IRMA:

Step 1. Exchange USD for an established one-to-one stablecoin, say USDT.

Step 2. Swap USDT for IRMA, either directly using the IRMA smart contract, or using an independent Decentralized Exchange (DEX).

Arbitrageurs are going to take advantage of the difference between the issuance price from the redemption price. During fiat inflation, the issuance price is mostly higher than the redemption price (in units of backing). An arbitrageur can sell IRMA at a slightly lower price than the issuance price, at the same time bid for the backing at a slightly higher price than the redemption price; thereby gaining more IRMA in the process. This is OK.

## Initial set of stablecoin backings

We want to back IRMA with three stablecoins initially, as a kind of test. All three stablecoins must exist in Solana. All of USDT, USDC, and PYUSD exist in Solana. However, PYUSD is not the third largest in terms of market cap. The plan is to launch IRMA paired with SIX top stablecoins in Solana.
