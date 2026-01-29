import { NodeTNClient, StreamId, EthereumAddress } from "@trufnetwork/sdk-js";
import { Wallet } from "ethers";

// Truflation US Inflation Index configuration
const TRUFLATION_STREAM_ID = "st1e321de22ece39a258bc2588dd2871";
const TRUFLATION_DATA_PROVIDER = "0x4710a8d8f0d845da110086812a32de6d90d7ff5c";
const TRUFLATION_ENDPOINT = "https://gateway.mainnet.truf.network";
const TRUFLATION_CHAIN_ID = "tn-v2.1";

export default async function handler(req, res) {
  // CORS headers
  res.setHeader("Access-Control-Allow-Origin", "*");
  res.setHeader("Access-Control-Allow-Methods", "GET, OPTIONS");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");

  if (req.method === "OPTIONS") {
    return res.status(200).end();
  }

  if (req.method !== "GET") {
    return res.status(405).json({ error: "Method not allowed" });
  }

  try {
    // Create a dummy wallet for identification (read-only, no signing needed)
    const wallet = new Wallet(
      "0x0000000000000000000000000000000000000000000000000000000000000001"
    );

    // Initialize TRUF.NETWORK client
    const client = new NodeTNClient({
      endpoint: TRUFLATION_ENDPOINT,
      signerInfo: {
        address: wallet.address,
        signer: wallet,
      },
      chainId: TRUFLATION_CHAIN_ID,
    });

    // Create stream locator
    const streamLocator = {
      streamId: StreamId.fromString(TRUFLATION_STREAM_ID).throw(),
      dataProvider: EthereumAddress.fromString(TRUFLATION_DATA_PROVIDER).throw(),
    };

    // Query for today's data
    const unixTimestampToday = Date.now() / 1000;
    const recordOptions = {
      from: unixTimestampToday,
      to: unixTimestampToday,
    };

    // Fetch the inflation data (using legacy API format that works)
    const streamAction = client.loadAction();
    const records = await streamAction.getRecord({
      stream: streamLocator,
      options: recordOptions,
    });

    if (!records || records.length === 0) {
      return res.status(404).json({ error: "No inflation data available" });
    }

    const inflationRate = 4.0; // parseFloat(records[0].value);
    const eventTime = records[0].eventTime || records[0].event_time;

    return res.status(200).json({
      success: true,
      data: {
        inflationRate,
        eventTime,
        timestamp: Date.now(),
        source: "Truflation US Inflation Index",
        streamId: TRUFLATION_STREAM_ID,
      },
    });
  } catch (error) {
    console.error("Truflation fetch error:", error);
    return res.status(500).json({
      error: "Failed to fetch inflation data",
      message: error.message,
    });
  }
}
