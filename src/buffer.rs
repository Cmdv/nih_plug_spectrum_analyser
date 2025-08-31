use triple_buffer::TripleBuffer;

// How many samples to keep for visualisation
const WAVEFORM_BUFFER_SIZE: usize = 2048;

pub struct WaveformBuffer {
    // Triple buffer for the lock-free communication
    // The inne Vec<f32> holds our audio samples
    producer: triple_buffer::Input<Vec<f32>>,
    consumer: triple_buffer::Output<Vec<f32>>,

    // Temporary buffer for collecting sample (audio thread only)
    temp_buffer: Vec<f32>,

    // Position in temp_buffer
    write_position: usize,
}

impl WaveformBuffer {
    pub fn new() -> Self {
        // Create the triple buffer with initial empty data
        let buffer = vec![0.0; WAVEFORM_BUFFER_SIZE];
        let (producer, consumer) = TripleBuffer::new(&buffer).split();

        Self {
            producer: producer,
            consumer: consumer,
            temp_buffer: vec![0.0; WAVEFORM_BUFFER_SIZE],
            write_position: 0,
        }


        // TODO: Initialize the struct
        // - Set up temp_buffer with the same size
        // - Initialize write_position to 0
    }

    // Called from audio thread - NO ALLOCATIONS!
    pub fn write_samples(&mut self, samples: &[f32]) {
        // TODO:
        // 1. Copy samples into temp_buffer at write_position
        // 2. Update write_position
        // 3. If buffer is full, send it to the consumer and reset
        //
        // Hint: use producer.write() to send data
        // Remember: this runs 48,000+ times per second!
    }

    // Called from UI thread - can allocate
    pub fn read_samples(&mut self) -> Vec<f32> {
        // TODO:
        // 1. Check if new data is available with consumer.read()
        // 2. Return a clone of the data (or interpolated/downsampled version)
    }
}
