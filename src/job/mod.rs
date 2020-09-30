use crate::channel::Channel;

struct WrappedJob<T: Job + 'static> {
    job: T,
    id: usize,
}

trait Job {
    type Result;

    fn poll(&mut self) -> Option<Self::Result>;
}

struct JobServer<T: Job + 'static, N: 'static + heapless::ArrayLength<T>> {
    jobs: heapless::Vec<WrappedJob<T>, N>,
    id_counter: usize,
}

impl<T: Job + 'static, N: 'static + heapless::ArrayLength<T>> JobServer<T, N> {
    fn try_post(&mut self, job: T) -> Result<(), T> {
        let next_id = self.id_counter;
        self.id_counter += 1;
        self.jobs.push(WrappedJob {
            job,
            id: next_id,
        }).map_err(|e| {
            Err(e.job)
        })
    }
}

