# lanpilot_discovery

Browses `_lanpilot._udp` with NWBrowser on iOS and streams raw results over the EventChannel `lanpilot/discovery`.
Events are maps: `found` (fullname, txt, addrs, port), `lost` (fullname) and `error` (message); all are untrusted and must be validated by core.
