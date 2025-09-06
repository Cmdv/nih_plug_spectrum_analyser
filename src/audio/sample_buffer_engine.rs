use crate::audio::constants::WAVEFORM_BUFFER_SIZE;
use triple_buffer::TripleBuffer;

// How many samples to keep for visualisation

pub struct SampleBufferEngine {
    // Triple buffer for the lock-free communication
    // The inne Vec<f32> holds our audio samples
    producer: triple_buffer::Input<Vec<f32>>,
    consumer: triple_buffer::Output<Vec<f32>>,

    // Temporary buffer for collecting sample (audio thread only)
    temp_buffer: Vec<f32>,

    // Position in temp_buffer
    write_position: usize,
}

impl SampleBufferEngine {
    pub fn new() -> Self {
        // Create the triple buffer with initial empty data
        let buffer = vec![0.0; WAVEFORM_BUFFER_SIZE];
        let (producer, consumer) = TripleBuffer::new(&buffer).split();

        Self {
            producer,
            consumer,
            temp_buffer: vec![0.0; WAVEFORM_BUFFER_SIZE],
            write_position: 0,
        }
    }

    // Called from audio thread - NO ALLOCATIONS!
    pub fn write_samples(&mut self, samples: &[f32]) {
        for &sample in samples {
            self.temp_buffer[self.write_position] = sample;
            self.write_position += 1;
            if self.write_position >= WAVEFORM_BUFFER_SIZE {
                // Buffer is full, send it to consumer
                self.producer.write(self.temp_buffer.clone());
                self.write_position = 0;
            }
        }
    }

    // Called from UI thread - can allocate
    pub fn read_samples(&mut self) -> Vec<f32> {
        self.consumer.read().clone()
    }
}
