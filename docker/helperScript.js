const fs = require('fs')

function calculateAverageDurationAndCountStates (jsonFilePath) {
  fs.readFile(jsonFilePath, 'utf8', (err, data) => {
    if (err) {
      console.error('Error reading the file:', err)
      return
    }
    try {
      const jsonData = JSON.parse(data)
      const durations = []
      const stateCounts = {
        FAILED: 0,
        OK: 0,
        INFORMATIONAL: 0,
        NON_STRICT: 0
      }
      const sectionDurations = {}
      let maxDuration = 0
      let mostTimeConsumingCase = null
      let mostTimeConsumingCaseBehavior = null
      let maxOkDuration = 0
      let mostTimeConsumingOkCase = null

      const serverData = jsonData['Rust WebSocket Server']
      if (!serverData) {
        console.error("No 'Rust WebSocket Server' data found.")
        return
      }

      for (const key in serverData) {
        if (serverData.hasOwnProperty(key)) {
          const nestedData = serverData[key]
          const section = key.split('.')[0]
          if (!sectionDurations[section]) {
            sectionDurations[section] = []
          }

          if (typeof nestedData.duration === 'number') {
            durations.push(nestedData.duration)
            sectionDurations[section].push(nestedData.duration)

            if (nestedData.duration > maxDuration) {
              maxDuration = nestedData.duration
              mostTimeConsumingCase = key
              mostTimeConsumingCaseBehavior = nestedData.behavior
            }

            if (
              nestedData.behavior === 'OK' &&
              nestedData.duration > maxOkDuration
            ) {
              maxOkDuration = nestedData.duration
              mostTimeConsumingOkCase = key
            }
          }
          if (typeof nestedData.behavior === 'string') {
            switch (nestedData.behavior.toUpperCase()) {
              case 'FAILED':
                stateCounts.FAILED++
                break
              case 'OK':
                stateCounts.OK++
                break
              case 'INFORMATIONAL':
                stateCounts.INFORMATIONAL++
                break
              case 'NON-STRICT':
                stateCounts.NON_STRICT++
                break
              default:
                break
            }
          }
        }
      }

      if (durations.length === 0) {
        console.log('No durations found in the JSON file.')
        return
      }

      const totalDuration = durations.reduce(
        (acc, duration) => acc + duration,
        0
      )
      const averageDuration = totalDuration / durations.length

      console.log(`Overall Average Duration: ${averageDuration}`)
      console.log('State Counts:', stateCounts)

      for (const section in sectionDurations) {
        const sectionTotal = sectionDurations[section].reduce(
          (acc, duration) => acc + duration,
          0
        )
        const sectionAverage = sectionTotal / sectionDurations[section].length
        console.log(
          `Average Duration for Section ${section}: ${sectionAverage}`
        )
      }

      if (mostTimeConsumingCase !== null) {
        console.log(
          `Most Time-Consuming Case: ${mostTimeConsumingCase} with duration ${maxDuration} and behavior ${mostTimeConsumingCaseBehavior}`
        )
      }

      if (mostTimeConsumingOkCase !== null) {
        console.log(
          `Most Time-Consuming Case with OK Behavior: ${mostTimeConsumingOkCase} with duration ${maxOkDuration}`
        )
      }
    } catch (parseError) {
      console.error('Error parsing JSON:', parseError)
    }
  })
}
calculateAverageDurationAndCountStates(
  '***/docker/reports/index.json'
)
