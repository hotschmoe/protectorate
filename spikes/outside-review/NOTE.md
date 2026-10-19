# Note on these reviews

Both reviews were prompted with the earlier ethos wording ("lightweight is
everything"). The user later clarified that lightweight applies only to how we
interact with the harnesses. The orchestration layer should be as robust as
correctness needs. Several cuts they propose (drop Connect/protobuf, drop
tsnet, skip containers) were argued on minimalism grounds alone and should be
re-weighed against robustness. Their points on the trust model, stale-quota
handling, handoff memory and per-host gating stand regardless.
