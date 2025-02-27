import type { Amount } from "./Amount";
import type { NonFungibleId } from "./NonFungibleId";
import type { ResourceAddress } from "./ResourceAddress";
export type ResourceContainer = {
    Fungible: {
        address: ResourceAddress;
        amount: Amount;
        locked_amount: Amount;
    };
} | {
    NonFungible: {
        address: ResourceAddress;
        token_ids: Array<NonFungibleId>;
        locked_token_ids: Array<NonFungibleId>;
    };
} | {
    Confidential: {
        address: ResourceAddress;
        commitments: Record<string, ConfidentialOutput>;
        revealed_amount: Amount;
        locked_commitments: Record<string, ConfidentialOutput>;
        locked_revealed_amount: Amount;
    };
};
import { ConfidentialOutput } from "./ConfidentialOutput";
