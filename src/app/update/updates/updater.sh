#!/bin/sh
while kill -0 {PID} 2>/dev/null; do sleep 0.5; done
mv {NEW} {OLD}
chmod +x {OLD}
exec {OLD}
