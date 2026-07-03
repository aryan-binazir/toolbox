# Replace overlapping runs for the same routine

If a routine becomes due while its previous run is still active, AI Scheduler will cancel the older run and start the newer scheduled run. The app treats overlapping schedules for the same routine as unintentional, so a still-running previous execution is stale rather than a reason to skip the fresh occurrence. Run history must show the older run as superseded so the replacement is auditable.
