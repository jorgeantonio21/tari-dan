// This file was generated by [ts-rs](https://github.com/Aleph-Alpha/ts-rs). Do not edit this file manually.
import type { Epoch } from "../Epoch";
import type { VNCommitteeShardInfo } from "./VNCommitteeShardInfo";

export interface GetNetworkCommitteeResponse {
  current_epoch: Epoch;
  committees: Array<VNCommitteeShardInfo>;
}
